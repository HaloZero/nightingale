.PHONY: restart export-lrc

# Rebuilds the frontend + server (release) and restarts the running
# instance, logging to ~/.nightingale/nightingale.log (appended across
# restarts, not truncated -- see scripts/restart-server.sh).
restart:
	./scripts/restart-server.sh

# Exports a song's cached transcript as an Enhanced LRC file you can save
# wherever you want (Nightingale itself never writes an .lrc back into your
# music folder). Pass args via ARGS, e.g.:
#   make export-lrc ARGS="--search toxic britney"
#   make export-lrc ARGS="000a1b2b2c618807bd466e2986e6db90 -o Toxic.lrc"
# See scripts/export_lrc.py for the full --help.
export-lrc:
	python3 scripts/export_lrc.py $(ARGS)
