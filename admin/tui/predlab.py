#!/usr/bin/env python3
"""
Prediction Lab — PredLab (Club Admin TUI)
Beautiful interactive terminal tool for the school Prediction Markets Club.
Manages dual-platform paper keys, club overview, and leaderboards.
"""

import sys
import time
import sqlite3
import os
from datetime import datetime
from pathlib import Path

try:
    import requests
    from rich.console import Console
    from rich.table import Table
    from rich.panel import Panel
    from rich.prompt import Prompt
    from rich.live import Live
    from rich.text import Text
except ImportError:
    print("Missing rich or requests. Run:")
    print("  nix-shell -p python3Packages.rich python3Packages.requests --run predlab")
    sys.exit(1)

# ------------------------------------------------------------------
# Central club student registry (local, private to the admin TUI)
# ------------------------------------------------------------------
DB_PATH = Path.home() / ".predlab" / "students.db"

def init_db():
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS students (
            username TEXT PRIMARY KEY,
            display_name TEXT,
            poly_key TEXT,
            kalshi_key TEXT,
            created_at TEXT
        )
    """)
    conn.commit()
    conn.close()

def save_student(username: str, display_name: str, poly_key: str, kalshi_key: str):
    init_db()
    conn = sqlite3.connect(DB_PATH)
    conn.execute(
        "INSERT OR REPLACE INTO students (username, display_name, poly_key, kalshi_key, created_at) VALUES (?, ?, ?, ?, ?)",
        (username, display_name, poly_key, kalshi_key, datetime.now().isoformat())
    )
    conn.commit()
    conn.close()

def list_students():
    init_db()
    conn = sqlite3.connect(DB_PATH)
    rows = conn.execute("SELECT username, display_name, poly_key, kalshi_key, created_at FROM students ORDER BY created_at DESC").fetchall()
    conn.close()
    return rows

# ------------------------------------------------------------------
# Backend URLs
# ------------------------------------------------------------------

POLY_URL = "http://localhost:8001"
KALSHI_URL = "http://localhost:8002"
console = Console()

def check_health():
    statuses = {}
    for name, url in [("poly", POLY_URL), ("kalshi", KALSHI_URL)]:
        try:
            r = requests.get(f"{url}/health", timeout=2)
            statuses[name] = "OK" if r.status_code == 200 else "DOWN"
        except Exception:
            statuses[name] = "DOWN"
    return statuses

def create_paper_key(username: str, sim: str, admin_secret: str | None = None) -> str:
    """Create a paper identity on one simulator.

    For Polymarket, pass admin_secret to satisfy the X-Admin-Secret header
    required by the /admin/create-paper-key endpoint.
    """
    if sim == "poly":
        url = f"{POLY_URL}/admin/create-paper-key?username={username}"
        headers = {}
        if admin_secret:
            headers["X-Admin-Secret"] = admin_secret

        resp = requests.post(url, headers=headers, timeout=10)
        if resp.status_code == 200:
            return resp.text.strip()
        raise Exception(f"Polymarket error: {resp.text}")
    else:
        url = f"{KALSHI_URL}/trade-api/v2/api_keys/generate"
        resp = requests.post(url, json={"name": username}, timeout=10)
        if resp.status_code == 200:
            data = resp.json()
            key_id = data.get("api_key_id", "unknown")
            priv = data.get("private_key", "")
            if priv:
                return f"key_id={key_id}\n\nPRIVATE PEM (save once):\n{priv}"
            return f"key_id={key_id}"
        raise Exception(f"Kalshi error: {resp.text}")

def main_menu():
    console.clear()
    health = check_health()
    p = "[green]●[/green] POLY" if health["poly"] == "OK" else "[red]●[/red] POLY"
    k = "[green]●[/green] KALSHI" if health["kalshi"] == "OK" else "[red]●[/red] KALSHI"

    mode_label = "ADMIN" if IS_ADMIN else "READ-ONLY"
    console.print(Panel(Text(f"PREDICTION LAB  •  PREDLAB  •  {mode_label}", style="bold green"), border_style="green"))
    console.print(f"{p}    {k}    [dim]{datetime.now().strftime('%H:%M')}[/dim]\n")

    table = Table.grid(padding=(0, 2))

    if IS_ADMIN:
        table.add_row("[bold cyan]1[/]", "Create new student + issue keys for BOTH simulators")
        table.add_row("[bold cyan]2[/]", "List all students + their keys")
        table.add_row("[bold cyan]3[/]", "Club Overview (everyone can see this)")
        table.add_row("[bold cyan]4[/]", "Live Leaderboard")
        table.add_row("[bold cyan]5[/]", "Reset a user's balance")
        table.add_row("[bold cyan]6[/]", "Force resolve a market (teaching)")
        table.add_row("[bold cyan]7[/]", "Backend health check")
        table.add_row("[bold cyan]q[/]", "Quit")
        choices = ["1","2","3","4","5","6","7","q"]
    else:
        # Student / read-only mode
        table.add_row("[bold cyan]1[/]", "Club Overview")
        table.add_row("[bold cyan]2[/]", "Live Leaderboard")
        table.add_row("[bold cyan]3[/]", "My Positions (using your key)")
        table.add_row("[bold cyan]4[/]", "Backend health check")
        table.add_row("[bold cyan]q[/]", "Quit")
        choices = ["1","2","3","4","q"]

    title = "Admin Menu" if IS_ADMIN else "Student Menu (Overview only)"
    console.print(Panel(table, title=title, border_style="dim"))
    return Prompt.ask("Select", choices=choices, default="1" if not IS_ADMIN else "3")

def create_dual_student_flow():
    """The main onboarding flow the club will use."""
    console.clear()
    console.print(Panel("CREATE STUDENT — KEYS FOR BOTH POLYMARKET AND KALSHI", style="bold green"))

    username = Prompt.ask("Username (short, e.g. alice_quant, bob_maker)", default="new_student")
    display = Prompt.ask("Display name (optional, shown in leaderboard)", default=username)

    starting_balance = Prompt.ask("Starting paper balance (dollars)", default="25000")

    # The Polymarket admin endpoint requires the secret
    admin_secret = Prompt.ask("Admin secret (X-Admin-Secret)", default="dev-only-change-me")

    console.print(f"\n[cyan]Creating '{username}' on both simulators with ${starting_balance} paper each...[/cyan]\n")

    poly_key = ""
    kalshi_key = ""

    try:
        with console.status("Creating on Polymarket-sim..."):
            poly_key = create_paper_key(username, "poly", admin_secret)
        console.print("[green]✓[/green] Polymarket key created")

        with console.status("Creating on Kalshi-sim..."):
            kalshi_key = create_paper_key(username, "kalshi")
        console.print("[green]✓[/green] Kalshi key created")

        save_student(username, display, poly_key, kalshi_key)

        console.print()
        result_panel = Panel(
            f"[bold]Username:[/bold] {username}\n"
            f"[bold]Display name:[/bold] {display}\n\n"
            f"[bold green]Polymarket Key (use as POLY_API_KEY):[/bold green]\n{poly_key}\n\n"
            f"[bold green]Kalshi Key (use as X-Kalshi-Sim-User or in SDK):[/bold green]\n{kalshi_key}",
            title=f"[green]Student created: {username}[/green]",
            border_style="green"
        )
        console.print(result_panel)

        console.print("\n[dim]Give the student their username + the two keys above.[/dim]")
        console.print("[dim]They can now trade on either (or both) simulators using the same identity.[/dim]")

    except Exception as e:
        console.print(f"\n[red]Error during creation:[/red] {e}")
        if "Invalid admin secret" in str(e):
            console.print("[yellow]Tip: The default admin secret is usually 'dev-only-change-me'.[/yellow]")
            console.print("[yellow]      Check the .env of polymarket-sim if you changed it.[/yellow]")
        console.print("[yellow]You may need to manually clean up partial accounts on the failing simulator.[/yellow]")

    Prompt.ask("\nPress Enter to return to menu", default="")

def leaderboard_flow():
    """Shows students from our local registry and tries to fetch live positions."""
    console.clear()
    console.print("[bold green]PREDLAB LEADERBOARD — All Club Students[/] (Ctrl+C to stop)\n")

    students = list_students()

    def build():
        t = Table(title="PredLab — Live Positions Across Both Markets")
        t.add_column("Student", style="cyan")
        t.add_column("Poly Balance", justify="right")
        t.add_column("Kalshi Balance", justify="right")
        t.add_column("Total P&L (est)", justify="right", style="green")

        if not students:
            t.add_row("No students yet — use option 1 to create some", "", "", "")
            return t

        for row in students:
            uname, dname, pkey, kkey, _ = row
            display = dname or uname

            poly_bal = "—"
            kal_bal = "—"

            # Try to get real balances using the keys we issued
            if pkey:
                try:
                    r = requests.get(f"{POLY_URL}/positions", headers={"POLY_API_KEY": pkey}, timeout=3)
                    if r.status_code == 200:
                        data = r.json()
                        if data:
                            poly_bal = str(data[0].get("balance", "?"))
                except Exception:
                    pass

            if kkey:
                try:
                    r = requests.get(f"{KALSHI_URL}/trade-api/v2/portfolio/balance", headers={"X-Kalshi-Sim-User": kkey}, timeout=3)
                    if r.status_code == 200:
                        data = r.json()
                        kal_bal = str(data.get("balance", "?"))
                except Exception:
                    pass

            t.add_row(display, poly_bal, kal_bal, "—")

        return t

    try:
        with Live(build(), refresh_per_second=1.2, screen=True) as live:
            while True:
                time.sleep(2.5)
                live.update(build())
    except KeyboardInterrupt:
        pass

def reset_flow():
    sim = Prompt.ask("Simulator", choices=["poly","kalshi"])
    user = Prompt.ask("Username")
    secret = Prompt.ask("Admin secret", default="dev-only-change-me")
    if sim == "poly":
        r = requests.post(f"{POLY_URL}/admin/reset-balance?username={user}", headers={"X-Admin-Secret": secret})
    else:
        r = requests.post(f"{KALSHI_URL}/trade-api/v2/admin/reset-user?username={user}", headers={"X-Kalshi-Sim-Admin": secret})
    console.print(Panel(r.text[:300], title="Result"))

def resolve_flow():
    sim = Prompt.ask("Simulator", choices=["poly","kalshi"])
    m = Prompt.ask("Market / ticker")
    res = Prompt.ask("Resolution", choices=["yes","no"])
    secret = Prompt.ask("Admin secret", default="dev-only-change-me")
    if sim == "poly":
        r = requests.post(f"{POLY_URL}/admin/force-resolve?market_id={m}&resolution={res}", headers={"X-Admin-Secret": secret})
    else:
        r = requests.post(f"{KALSHI_URL}/trade-api/v2/admin/resolve/{m}?result={res}", headers={"X-Kalshi-Sim-Admin": secret})
    console.print(Panel(r.text[:300]))

def list_students_flow():
    console.clear()
    students = list_students()
    if not students:
        console.print("[yellow]No students created yet. Use option 1 to create the first one.[/yellow]")
        Prompt.ask("")
        return

    table = Table(title="Club Students — Dual Simulator Access")
    table.add_column("Username", style="cyan")
    table.add_column("Display Name")
    table.add_column("Polymarket Key (truncated)")
    table.add_column("Kalshi Key (truncated)")
    table.add_column("Created")

    for row in students:
        uname, dname, pkey, kkey, created = row
        table.add_row(
            uname,
            dname or uname,
            (pkey or "")[:35] + "..." if pkey else "—",
            (kkey or "")[:35] + "..." if kkey else "—",
            created[:16]
        )

    console.print(table)
    console.print("\n[dim]Students can use either key depending on which market they want to trade.[/dim]")
    Prompt.ask("")

# ------------------------------------------------------------------
# Global mode
# ------------------------------------------------------------------
CURRENT_KEY = None          # the paper key the user provided
IS_ADMIN = False            # whether they have admin rights


def detect_mode():
    """Ask for a paper key. Blank = full admin mode."""
    global CURRENT_KEY, IS_ADMIN

    console.clear()
    console.print(Panel(
        Text("PREDICTION LAB — CLUB TERMINAL", style="bold green"),
        border_style="green"
    ))
    console.print()

    key = Prompt.ask(
        "Enter your paper API key (or press Enter for Admin / Club Overview)",
        default=""
    ).strip()

    if not key:
        # Admin mode (local machine, trusted)
        IS_ADMIN = True
        CURRENT_KEY = None
        console.print("[green]Running in Admin mode — full controls enabled.[/green]")
    else:
        IS_ADMIN = False
        CURRENT_KEY = key
        console.print("[cyan]Running in Student / Read-only mode — Overview + Leaderboard only.[/cyan]")

    Prompt.ask("Press Enter to continue", default="")


def club_overview_flow():
    """Public overview screen — readable by anyone with a paper key or in admin mode."""
    console.clear()
    console.print(Panel("CLUB OVERVIEW — PREDICTION LAB", style="bold green"))

    students = list_students()

    # Aggregate simple stats
    total_students = len(students)
    poly_total = 0
    kal_total = 0

    for row in students:
        uname, _, pkey, kkey, _ = row
        try:
            if pkey:
                r = requests.get(f"{POLY_URL}/positions", headers={"POLY_API_KEY": pkey}, timeout=4)
                if r.status_code == 200:
                    for p in r.json():
                        poly_total += float(p.get("balance", 0))
            if kkey:
                r = requests.get(f"{KALSHI_URL}/trade-api/v2/portfolio/balance", headers={"X-Kalshi-Sim-User": kkey}, timeout=4)
                if r.status_code == 200:
                    bal = r.json().get("balance", 0)
                    kal_total += float(bal)
        except Exception:
            pass

    total_paper = poly_total + kal_total

    table = Table(title="Club Snapshot")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", justify="right")

    table.add_row("Registered Students", str(total_students))
    table.add_row("Total Paper on Polymarket", f"${poly_total:,.0f}")
    table.add_row("Total Paper on Kalshi", f"${kal_total:,.0f}")
    table.add_row("Grand Total Paper", f"${total_paper:,.0f}")

    console.print(table)
    console.print("\n[dim]This view is available to any club member with a paper key.[/dim]")
    console.print("[dim]Admin mode unlocks creation, resets, and full key management.[/dim]")

    Prompt.ask("")


def main():
    detect_mode()

    while True:
        choice = main_menu()

        # Student / Read-only mode
        if not IS_ADMIN:
            if choice == "1":
                club_overview_flow()
            elif choice == "2":
                leaderboard_flow()
            elif choice == "3":
                console.print("[yellow]My Positions view coming soon — use the website for now with your key.[/yellow]")
                Prompt.ask("")
            elif choice == "4":
                h = check_health()
                console.print(Panel(f"POLY: {h['poly']}   KALSHI: {h['kalshi']}"))
                Prompt.ask("")
            elif choice == "q":
                break
            continue

        # Admin mode
        if choice == "1":
            create_dual_student_flow()
        elif choice == "2":
            list_students_flow()
        elif choice == "3":
            club_overview_flow()
        elif choice == "4":
            leaderboard_flow()
        elif choice == "5":
            reset_flow()
        elif choice == "6":
            resolve_flow()
        elif choice == "7":
            h = check_health()
            console.print(Panel(f"POLY: {h['poly']}   KALSHI: {h['kalshi']}"))
            Prompt.ask("")
        elif choice == "q":
            break

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nBye")
