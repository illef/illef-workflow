INSTALL_DIR  := /usr/local/bin
SERVICE_DIR  := $(HOME)/.config/systemd/user
SERVICE_FILE := illef-workflow.service

.PHONY: build install install\:tui uninstall

build:
	cargo build --release

install: build
	sudo install -Dm755 target/release/illef-workflow-runner $(INSTALL_DIR)/illef-workflow-runner
	sudo install -Dm755 target/release/illef-workflow-tui   $(INSTALL_DIR)/illef-workflow-tui
	install -Dm644 $(SERVICE_FILE) $(SERVICE_DIR)/$(SERVICE_FILE)
	systemctl --user daemon-reload
	@echo ""
	@echo "Done. To enable and start the runner:"
	@echo "  systemctl --user enable --now illef-workflow"

install\:tui: build
	sudo install -Dm755 target/release/illef-workflow-tui $(INSTALL_DIR)/illef-workflow-tui

uninstall:
	systemctl --user disable --now illef-workflow || true
	rm -f $(INSTALL_DIR)/illef-workflow-runner
	rm -f $(INSTALL_DIR)/illef-workflow-tui
	rm -f $(SERVICE_DIR)/$(SERVICE_FILE)
	systemctl --user daemon-reload
