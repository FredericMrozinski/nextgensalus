#!/bin/sh
# Creates the virtualenv with OpenSlide (needs Python 3.10+) for the backend.
cd "$(dirname "$0")" || exit 1
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
