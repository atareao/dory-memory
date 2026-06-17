"""Dory Memory Provider Plugin for Hermes Agent.

Calls the Dory Memory Rust backend over HTTP.
Namespace is auto-derived from the Hermes profile name.
"""

import json
import os
import threading
from pathlib import Path
from urllib.parse import urljoin

import requests

from agent.memory_provider import MemoryProvider

API_URL_DEFAULT = "http://localhost:5005"


class DoryMemoryProvider(MemoryProvider):
    def __init__(self):
        self._api_url = API_URL_DEFAULT
        self._namespace = "default"
        self._session_id = ""
        self._sync_thread = None

    @property
    def name(self) -> str:
        return "dory"

    def is_available(self) -> bool:
        return bool(os.environ.get("DORY_API_URL", API_URL_DEFAULT))

    def initialize(self, session_id: str, **kwargs) -> None:
        self._api_url = os.environ.get("DORY_API_URL", API_URL_DEFAULT).rstrip("/")
        self._session_id = session_id

        hermes_home = kwargs.get("hermes_home", "")
        self._namespace = self._derive_namespace(hermes_home, session_id)

    @staticmethod
    def _derive_namespace(hermes_home: str, session_id: str) -> str:
        if hermes_home:
            profile = Path(hermes_home).name
            if profile:
                return profile
        return session_id or "default"

    def _post(self, path: str, payload: dict) -> dict | None:
        url = urljoin(self._api_url + "/", path.lstrip("/"))
        try:
            resp = requests.post(url, json=payload, timeout=10)
            resp.raise_for_status()
            return resp.json() if resp.status_code != 204 else None
        except requests.RequestException as e:
            return {"error": str(e)}

    def _get(self, path: str) -> dict | None:
        url = urljoin(self._api_url + "/", path.lstrip("/"))
        try:
            resp = requests.get(url, timeout=10)
            resp.raise_for_status()
            return resp.json() if resp.status_code != 204 else None
        except requests.RequestException as e:
            return {"error": str(e)}

    def get_tool_schemas(self):
        return [
            {
                "name": "recall",
                "description": "Semantic + full-text search for memories",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What to search for",
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Filter by tags like status:todo",
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max results",
                            "default": 10,
                        },
                    },
                    "required": ["query"],
                },
            },
            {
                "name": "sweep",
                "description": "Check pending tasks that haven't been accessed in 24h",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": [],
                },
            },
            {
                "name": "search_temporal",
                "description": "Recall memories from a time window",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start": {
                            "type": "string",
                            "description": "ISO datetime window start",
                        },
                        "end": {
                            "type": "string",
                            "description": "ISO datetime window end",
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "limit": {"type": "integer", "default": 10},
                    },
                    "required": [],
                },
            },
            {
                "name": "list_stale",
                "description": "List non-immortal memories with low importance and old access time — candidates for cleanup",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "older_than_hours": {
                            "type": "integer",
                            "description": "Age threshold in hours (default 72)",
                        },
                        "importance_below": {
                            "type": "number",
                            "description": "Importance ceiling (default 0.5)",
                        },
                        "limit": {"type": "integer", "default": 20},
                    },
                    "required": [],
                },
            },
            {
                "name": "purge",
                "description": "Batch delete memories matching filters. Immortal memories are protected.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "older_than_hours": {
                            "type": "integer",
                            "description": "Delete memories not accessed in this many hours",
                        },
                        "importance_below": {
                            "type": "number",
                            "description": "Delete memories with importance below this value",
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Only delete memories matching these tags",
                        },
                    },
                    "required": [],
                },
            },
            {
                "name": "stats",
                "description": "Show database statistics: total memories, immortal count, namespace breakdown, avg importance",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": [],
                },
            },
        ]

    def handle_tool_call(self, tool_name: str, args: dict, **kwargs):
        if tool_name == "recall":
            return self._handle_recall(args)
        if tool_name == "sweep":
            return self._handle_sweep()
        if tool_name == "search_temporal":
            return self._handle_search_temporal(args)
        if tool_name == "list_stale":
            return self._handle_list_stale(args)
        if tool_name == "purge":
            return self._handle_purge(args)
        if tool_name == "stats":
            return self._handle_stats()
        return {"error": f"Unknown tool: {tool_name}"}

    def _handle_recall(self, args: dict) -> str:
        result = self._post(
            "/v1/search",
            {
                "namespace": self._namespace,
                "query_text": args.get("query", ""),
                "tags": args.get("tags", []),
                "limit": args.get("limit", 10),
            },
        )
        if not result or "error" in (result or {}):
            return f"Recall failed: {result}"
        memories = result if isinstance(result, list) else result.get("data", result)
        if not memories:
            return "No memories found."
        lines = []
        for m in memories:
            tags = json.dumps(m.get("tags", []))
            lines.append(f"- [{m['importance']:.2f}] {m['content_l0']}  tags={tags}")
        return "\n".join(lines)

    def _handle_sweep(self) -> str:
        result = self._get(f"/v1/sweep/{self._namespace}")
        if not result or "error" in (result or {}):
            return f"Sweep failed: {result}"
        memories = result if isinstance(result, list) else result.get("data", result)
        if not memories:
            return "No stale tasks found."
        lines = []
        for m in memories:
            lines.append(
                f"- {m['content_l0']} (last accessed: {m['last_accessed_at']})"
            )
        return "\n".join(lines)

    def _handle_search_temporal(self, args: dict) -> str:
        payload = {
            "namespace": self._namespace,
            "tags": args.get("tags", []),
            "limit": args.get("limit", 10),
        }
        if args.get("start"):
            payload["start"] = args["start"]
        if args.get("end"):
            payload["end"] = args["end"]

        result = self._post("/v1/search/temporal", payload)
        if not result or "error" in (result or {}):
            return f"Temporal search failed: {result}"
        memories = result if isinstance(result, list) else result.get("data", result)
        if not memories:
            return "No memories in that time window."
        lines = []
        for m in memories:
            lines.append(f"- [{m['created_at']}] {m['content_l0']}")
        return "\n".join(lines)

    def _handle_list_stale(self, args: dict) -> str:
        result = self._post(
            "/v1/maintenance/stale",
            {
                "namespace": self._namespace,
                "older_than_hours": args.get("older_than_hours", 72),
                "importance_below": args.get("importance_below", 0.5),
                "limit": args.get("limit", 20),
            },
        )
        if not result or "error" in (result or {}):
            return f"List stale failed: {result}"
        memories = result if isinstance(result, list) else result.get("data", result)
        if not memories:
            return "No stale memories found."
        lines = []
        for m in memories:
            lines.append(
                f"- [{m['importance']:.2f}] {m['content_l0']}  (last: {m['last_accessed_at']})"
            )
        return "\n".join(lines)

    def _handle_purge(self, args: dict) -> str:
        payload = {"namespace": self._namespace}
        if args.get("older_than_hours"):
            payload["older_than_hours"] = args["older_than_hours"]
        if args.get("importance_below"):
            payload["importance_below"] = args["importance_below"]
        if args.get("tags"):
            payload["tags"] = args["tags"]

        result = self._post("/v1/batch/delete", payload)
        if not result or "error" in (result or {}):
            return f"Purge failed: {result}"
        deleted = result.get("deleted", 0)
        return f"Purged {deleted} memory record(s). Immortal memories were protected."

    def _handle_stats(self) -> str:
        result = self._get("/v1/stats")
        if not result or "error" in (result or {}):
            return f"Stats failed: {result}"
        lines = [
            f"Total memories: {result.get('total_memories', '?')}",
            f"Immortal: {result.get('immortal_count', '?')}",
            f"Namespaces: {result.get('namespace_count', '?')}",
            f"Avg importance: {result.get('avg_importance', '?'):.3f}",
            f"Low-importance candidates: {result.get('low_importance_count', '?')}",
        ]
        ns = result.get("namespaces", [])
        if ns:
            lines.append("Per namespace:")
            for n in ns:
                lines.append(f"  {n['name']}: {n['count']}")
        if result.get("oldest_memory"):
            lines.append(f"Oldest: {result['oldest_memory']}")
        if result.get("newest_memory"):
            lines.append(f"Newest: {result['newest_memory']}")
        return "\n".join(lines)

    def prefetch(self, query: str, *, session_id: str = "") -> str:
        if not query:
            return ""
        result = self._post(
            "/v1/search/budget",
            {
                "namespace": self._namespace,
                "query_text": query,
                "max_tokens": 2000,
            },
        )
        if not result:
            return ""
        memories = result if isinstance(result, list) else result.get("data", result)
        if not memories:
            return ""
        lines = []
        for m in memories:
            lines.append(f"- {m['content_l0']}")
        return "\n".join(lines)

    def sync_turn(
        self,
        user_content: str,
        assistant_content: str,
        *,
        session_id: str = "",
        messages=None,
    ):
        def _sync():
            try:
                self._post(
                    "/v1/memories",
                    {
                        "namespace": self._namespace,
                        "content_l0": f"User: {user_content[:200]}",
                        "content_l1": user_content,
                        "intent": "Task",
                        "is_immortal": False,
                        "tags": ["type:conversation", "role:user"],
                    },
                )
                self._post(
                    "/v1/memories",
                    {
                        "namespace": self._namespace,
                        "content_l0": f"Assistant: {assistant_content[:200]}",
                        "content_l1": assistant_content,
                        "intent": "Standard",
                        "is_immortal": False,
                        "tags": ["type:conversation", "role:assistant"],
                    },
                )
            except Exception as e:
                import logging

                logging.warning("Dory sync_turn failed: %s", e)

        if self._sync_thread and self._sync_thread.is_alive():
            self._sync_thread.join(timeout=5.0)
        self._sync_thread = threading.Thread(target=_sync, daemon=True)
        self._sync_thread.start()

    def on_session_end(self, messages) -> None:
        if messages:
            summary = (
                messages[-1].get("content", "")[:500]
                if isinstance(messages[-1], dict)
                else str(messages[-1])[:500]
            )
            self._post(
                "/v1/memories",
                {
                    "namespace": self._namespace,
                    "content_l0": f"Session summary: {summary[:200]}",
                    "content_l1": summary,
                    "intent": "Reference",
                    "is_immortal": True,
                    "tags": ["type:session_summary"],
                },
            )

    def get_config_schema(self):
        return [
            {
                "key": "api_url",
                "description": "Dory Memory backend URL",
                "default": API_URL_DEFAULT,
                "secret": False,
                "required": True,
                "env_var": "DORY_API_URL",
            },
            {
                "key": "default_namespace",
                "description": "Override the auto-derived namespace. Leave blank to use Hermes profile name.",
                "default": "",
                "secret": False,
                "required": False,
            },
        ]

    def save_config(self, values: dict, hermes_home: str) -> None:
        config_path = Path(hermes_home) / "dory.json"
        config_path.write_text(json.dumps(values, indent=2))


def register(ctx) -> None:
    ctx.register_memory_provider(DoryMemoryProvider())
