"""Dory CLI commands for Hermes Agent."""

import json
from pathlib import Path

import requests


def status(args):
    """Check Dory backend connectivity and stats."""
    api_url = getattr(args, "api_url", "http://localhost:5005")
    try:
        resp = requests.get(f"{api_url}/v1/sweep/default", timeout=5)
        if resp.status_code < 500:
            print(f"Dory backend at {api_url} is reachable.")
            print(f"Status code: {resp.status_code}")
        else:
            print(f"Dory backend returned {resp.status_code}")
    except requests.RequestException as e:
        print(f"Cannot reach Dory backend at {api_url}: {e}")


def config(args):
    """Show Dory plugin configuration."""
    hermes_home = getattr(args, "hermes_home", Path.home() / ".hermes")
    config_path = Path(hermes_home) / "dory.json"
    if config_path.exists():
        print(config_path.read_text())
    else:
        print("No dory.json config found. Run `hermes memory setup` first.")


def stats(args):
    """Fetch and display database statistics from Dory backend."""
    api_url = getattr(args, "api_url", "http://localhost:5005")
    try:
        resp = requests.get(f"{api_url}/v1/stats", timeout=5)
        resp.raise_for_status()
        data = resp.json()
        print(json.dumps(data, indent=2))
    except requests.RequestException as e:
        print(f"Failed to fetch stats: {e}")


def register_cli(subparser) -> None:
    subs = subparser.add_subparsers(dest="dory_command")
    subs.add_parser("status", help="Check Dory backend connectivity")
    subs.add_parser("config", help="Show Dory plugin configuration")
    subs.add_parser("stats", help="Show Dory database statistics")
    subparser.set_defaults(func=_dispatch)


def _dispatch(args):
    cmd = getattr(args, "dory_command", None)
    if cmd == "status":
        status(args)
    elif cmd == "config":
        config(args)
    elif cmd == "stats":
        stats(args)
    else:
        print("Usage: hermes dory <status|config|stats>")
