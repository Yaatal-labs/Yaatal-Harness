import argparse
import asyncio
import json
import sys
import urllib.error
import urllib.request

import websockets


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Register/login against the local Engine and run one voice-session smoke turn."
    )
    parser.add_argument("--base-url", default="http://localhost:5150")
    parser.add_argument("--ws-url", default="ws://localhost:5150/api/voice/session")
    parser.add_argument("--email", default="boplex-smoke@example.com")
    parser.add_argument("--password", default="dev-password-123")
    parser.add_argument("--name", default="BoPlex Smoke")
    parser.add_argument("--session-id", default="smoke-session-1")
    parser.add_argument("--persona", default="market-guide")
    parser.add_argument("--lang", default="wo")
    parser.add_argument("--market", default="SN-DKR")
    parser.add_argument(
        "--transcript",
        default="find white fabric near Sandaga",
        help="Search-like transcript hint to inject into the mock voice loop.",
    )
    parser.add_argument("--audio-base64", default="ZmFrZQ==")
    parser.add_argument("--timeout-seconds", type=float, default=5.0)
    parser.add_argument("--max-messages", type=int, default=8)
    return parser.parse_args()


def post_json(url: str, payload: dict) -> dict:
    request = urllib.request.Request(
        url=url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


def ensure_user(args: argparse.Namespace) -> None:
    register_payload = {
        "email": args.email,
        "password": args.password,
        "name": args.name,
    }
    register_url = f"{args.base_url}/api/auth/register"
    try:
        post_json(register_url, register_payload)
        print(f"registered: {args.email}")
    except urllib.error.HTTPError as error:
        if error.code not in {409, 422}:
            raise
        print(f"register skipped: {args.email} already exists or is not re-creatable")


def login(args: argparse.Namespace) -> str:
    login_payload = {"email": args.email, "password": args.password}
    login_url = f"{args.base_url}/api/auth/login"
    response = post_json(login_url, login_payload)
    token = response.get("token")
    if not token:
        raise RuntimeError(f"login response missing token: {response}")
    return token


async def run_smoke(args: argparse.Namespace, token: str) -> int:
    message_count = 0
    async with websockets.connect(
        args.ws_url,
        additional_headers={"Authorization": f"Bearer {token}"},
    ) as websocket:
        await websocket.send(
            json.dumps(
                {
                    "type": "session_config",
                    "session_id": args.session_id,
                    "persona": args.persona,
                    "lang": args.lang,
                    "market": args.market,
                }
            )
        )
        await websocket.send(
            json.dumps(
                {
                    "type": "audio_chunk",
                    "audio_base64": args.audio_base64,
                    "transcript_hint": args.transcript,
                }
            )
        )

        while message_count < args.max_messages:
            try:
                raw_message = await asyncio.wait_for(
                    websocket.recv(), timeout=args.timeout_seconds
                )
            except asyncio.TimeoutError:
                print("timeout waiting for next websocket message")
                break

            message_count += 1
            print(raw_message)

            try:
                parsed = json.loads(raw_message)
            except json.JSONDecodeError:
                continue

            if parsed.get("type") == "turn_end":
                break

    return message_count


def main() -> int:
    args = parse_args()
    ensure_user(args)
    token = login(args)
    print("login ok")
    message_count = asyncio.run(run_smoke(args, token))
    print(f"messages_received={message_count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
