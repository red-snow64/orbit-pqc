PYTHON = .venv/bin/python3

.PHONY: all venv build netns-setup run-experiments plots clean

all: venv build netns-setup run-experiments plots

venv:
	@test -d .venv || python3 -m venv .venv
	@$(PYTHON) -m pip install --upgrade pip
	@$(PYTHON) -m pip install -r python/requirements.txt

build:
	@echo "[*] Compiling Rust workspace in release mode..."
	cargo build --release

netns-setup:
	@echo "[*] Initializing network namespaces..."
	@chmod +x scripts/*.sh
	sudo bash ./scripts/setup_netns.sh

run-experiments:
	@echo "[*] Running empirical Monte Carlo sweeps..."
	sudo $(PYTHON) python/experiments/runner.py

plots:
	@echo "[*] Parsing summaries and generating IEEE vector PDF figures..."
	$(PYTHON) python/experiments/parser.py
	$(PYTHON) python/plots/generate_ieee_plots.py

clean:
	@echo "[*] Tearing down namespaces and cleaning artifacts..."
	sudo bash ./scripts/teardown_netns.sh 2>/dev/null || true
	cargo clean
	rm -rf datasets/*.csv figures/*.pdf datasets/summaries/*.csv