.PHONY: help restart rebuild redeploy export-lrc

.DEFAULT_GOAL := help

# Lists the targets below with their one-line descriptions.
help:
	@echo "restart      Stop and restart the already-built release server (no rebuild)"
	@echo "rebuild      Build the frontend + server (release), no restart"
	@echo "redeploy     Build the frontend + server (release), then restart"
	@echo "export-lrc   Export a song's cached transcript as an Enhanced LRC file"
	@echo "             ARGS=\"<file_hash>\" or ARGS=\"--search <title/artist> [-o <path>|--next-to-source]\""

# Stops and restarts the already-built release server, logging to
# ~/.nightingale/nightingale.log (appended across restarts, not truncated --
# see scripts/restart-server.sh). Does not rebuild; use `redeploy` for that.
restart:
	./scripts/restart-server.sh

# Rebuilds the frontend + server (release) without restarting the running
# instance -- see scripts/rebuild-server.sh.
rebuild:
	./scripts/rebuild-server.sh

# Rebuilds the frontend + server (release) and restarts the running
# instance -- see scripts/redeploy-server.sh.
redeploy:
	./scripts/redeploy-server.sh

# Exports a song's cached transcript as an Enhanced LRC file you can save
# wherever you want (Nightingale itself never writes an .lrc back into your
# music folder). Identify the song by exact file_hash or --search substring
# (title/artist), then either -o/--output a specific path, --next-to-source
# to write alongside the song's own audio/video file (same basename, refuses
# to overwrite an existing .lrc there unless --force is also passed), or
# omit both to print to stdout. Pass args via ARGS, e.g.:
#   make export-lrc ARGS="--search 'toxic britney'"
#   make export-lrc ARGS="--search 'toxic britney' -o Toxic.lrc"
#   make export-lrc ARGS="--search 'toxic britney' --next-to-source"
#   make export-lrc ARGS="000a1b2b2c618807bd466e2986e6db90 -o Toxic.lrc"
# See scripts/export_lrc.py --help for the full option list (e.g. --data-dir
# to point at a data directory other than the one resolved by default).
export-lrc:
	python3 scripts/export_lrc.py $(ARGS)
