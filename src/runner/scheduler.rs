use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{Local, Utc};
use cron::Schedule;
use std::str::FromStr;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::common::types::{AppConfig, WorkflowConfig};
use crate::runner::executor::execute_workflow;

#[derive(Debug)]
pub enum SchedulerCommand {
    Reload(AppConfig),
    Trigger(String), // workflow name
}

struct WorkflowState {
    running: bool,
    queue: VecDeque<()>,
}

impl WorkflowState {
    fn new() -> Self {
        Self {
            running: false,
            queue: VecDeque::new(),
        }
    }
}

pub fn start(
    initial_config: AppConfig,
    db: Arc<Mutex<rusqlite::Connection>>,
) -> (mpsc::Sender<SchedulerCommand>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(32);
    let handle = tokio::spawn(scheduler_loop(initial_config, db, rx));
    (tx, handle)
}

async fn scheduler_loop(
    initial_config: AppConfig,
    db: Arc<Mutex<rusqlite::Connection>>,
    mut rx: mpsc::Receiver<SchedulerCommand>,
) {
    let mut config = initial_config;
    let states: Arc<Mutex<HashMap<String, WorkflowState>>> =
        Arc::new(Mutex::new(HashMap::new()));

    loop {
        let next_wake = compute_next_wake(&config);
        let sleep_duration = match next_wake {
            Some(dur) => dur,
            None => std::time::Duration::from_secs(60),
        };

        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {
                fire_due_workflows(&config, Arc::clone(&db), Arc::clone(&states)).await;
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(SchedulerCommand::Reload(new_config)) => {
                        info!("config reloaded");
                        config = new_config;
                    }
                    Some(SchedulerCommand::Trigger(name)) => {
                        if let Some(wf) = config.workflows.iter().find(|w| w.name == name) {
                            trigger_workflow(wf.clone(), Arc::clone(&db), Arc::clone(&states)).await;
                        } else {
                            warn!(workflow = %name, "trigger requested for unknown workflow");
                        }
                    }
                    None => break,
                }
            }
        }
    }
}

fn compute_next_wake(config: &AppConfig) -> Option<std::time::Duration> {
    let now = Local::now();
    let mut earliest: Option<chrono::DateTime<Local>> = None;

    for wf in &config.workflows {
        let Ok(schedule) = Schedule::from_str(&normalize_cron(&wf.cron)) else {
            continue;
        };
        if let Some(next) = schedule.upcoming(Local).next() {
            earliest = Some(match earliest {
                Some(e) if next < e => next,
                Some(e) => e,
                None => next,
            });
        }
    }

    earliest.and_then(|t| {
        let diff = t - now;
        if diff.num_milliseconds() > 0 {
            Some(std::time::Duration::from_millis(diff.num_milliseconds() as u64))
        } else {
            Some(std::time::Duration::from_millis(100))
        }
    })
}

async fn fire_due_workflows(
    config: &AppConfig,
    db: Arc<Mutex<rusqlite::Connection>>,
    states: Arc<Mutex<HashMap<String, WorkflowState>>>,
) {
    let now = Local::now();

    for wf in &config.workflows {
        let Ok(schedule) = Schedule::from_str(&normalize_cron(&wf.cron)) else {
            error!(workflow = %wf.name, cron = %wf.cron, "invalid cron expression");
            continue;
        };

        let due = is_due(&schedule, now);

        if due {
            trigger_workflow(wf.clone(), Arc::clone(&db), Arc::clone(&states)).await;
        }
    }
}

fn is_due(schedule: &Schedule, now: chrono::DateTime<Local>) -> bool {
    // if the first scheduled time after window_start is <= now, it's due
    let window_start = now - chrono::Duration::seconds(5);
    if let Some(t) = schedule.after(&window_start).next() {
        return t <= now;
    }
    false
}

async fn trigger_workflow(
    wf: WorkflowConfig,
    db: Arc<Mutex<rusqlite::Connection>>,
    states: Arc<Mutex<HashMap<String, WorkflowState>>>,
) {
    let mut states_lock = states.lock().await;
    let state = states_lock.entry(wf.name.clone()).or_insert_with(WorkflowState::new);

    if state.running {
        info!(workflow = %wf.name, "already running, queuing");
        state.queue.push_back(());
        return;
    }

    state.running = true;
    drop(states_lock);

    let name = wf.name.clone();
    let script = wf.script.clone();
    let message_script = wf.message_script.clone();
    let states_clone = Arc::clone(&states);
    let db_clone = Arc::clone(&db);

    // using default notification config for simplicity
    tokio::spawn(async move {
        let notification = crate::common::types::NotificationConfig::default();

        if let Err(e) = execute_workflow(name.clone(), script, message_script, db_clone, notification).await {
            error!(workflow = %name, error = %e, "execution error");
        }

        let mut states_lock = states_clone.lock().await;
        if let Some(state) = states_lock.get_mut(&name) {
            state.running = false;
            if state.queue.pop_front().is_some() {
                // queued item found; mark running and let next cycle pick it up
                state.running = true;
                drop(states_lock);
                info!(workflow = %name, "queued execution will be triggered on next cycle");
            }
        }
    });
}

pub fn get_next_run(cron_expr: &str) -> Option<chrono::DateTime<Utc>> {
    Schedule::from_str(&normalize_cron(cron_expr))
        .ok()?
        .upcoming(Local)
        .next()
        .map(|t| t.with_timezone(&Utc))
}

/// Normalize standard 5-field cron to the 6-field format (with seconds) required by the cron crate.
pub fn normalize_cron(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    }
}
