# PredLab / NCSSM Prediction Markets Club — Telegram Plan

A simple, low-maintenance structure: **one broadcast channel** for signal, **one main group**
for chatter, plus a couple of optional rooms you only spin up if there's demand. Keep it small
until it's busy — an empty room reads worse than no room.

## The setup

| Space | Type | Who posts | Purpose |
|---|---|---|---|
| **PredLab Announcements** | Channel (broadcast) | Admins only | Competitions, deadlines, leaderboard milestones, downtime, AI policy notes. Low volume, high signal. Comments **on** so members can react. |
| **PredLab Lounge** | Group | Everyone | The hub: strategy talk, "why is my order stuck open," market calls, trash talk, help. |
| **PredLab Dev / API** (optional) | Group | Everyone | For people building bots/agents against the API. Code, rate-limit questions, the Hermes-on-Grok reference, schema chat. Split off only once API questions crowd the Lounge. |
| **Leaderboard bot feed** (optional) | Bot → channel | Bot | A daily/weekly auto-post of the top 10 from `https://predlab.teddytennant.com`. Nice-to-have, not required. |

**Linking:** Pin the channel link in the group and vice-versa. Put the channel link in the
club charter/deck, the `/start` page, and the GitHub README.

## Pinned message (Lounge group)

> 📌 **Welcome to PredLab — the NCSSM Prediction Markets Club paper-trading floor.**
> Fake money, real strategy. Everyone starts with $25,000.
> • Leaderboard → predlab.teddytennant.com
> • Need a key? Ask an admin here.
> • New? Read the 6-step guide: github.com/teddytennant/predlab
> • Announcements channel → [link]
> AI is **encouraged** — bots welcome. Be kind, no real-money talk, no spam.

## Roles & moderation

- **Admins** = club officers (the same people who hold `admin`/`owner` API keys). 2–3 is plenty.
- **Slow mode** in the Lounge during big competitions (10–30s) to keep it readable.
- **Pin** the active competition post + leaderboard link; unpin when it ends.
- **One rule that matters:** never paste a real `pm_paper_…` key or the `ADMIN_SECRET` in chat.
  If someone does, an admin resets that key immediately.

## Cadence

| When | Channel | Lounge |
|---|---|---|
| Launch | Announcement post | Welcome + pin |
| Weekly | "Standings + market of the week" | "Trade idea Friday" prompt |
| Competition start/end | Rules + winners | Hype + recap |
| Ad hoc | Downtime / resolved markets | Help, banter |

Keep the channel to **2–4 posts/week** max. The group self-sustains once ~15 active members
are in.

## Growth playbook

1. Seed it with the officers + 5–10 early members before announcing widely — a room with
   activity converts; an empty one doesn't.
2. Announce in the all-school channels / club fair with the QR to the channel.
3. Run a **launch competition** (see below) with a small prize — fastest way to get people
   to actually place a first order.
4. Cross-post leaderboard milestones ("first member past $30k") — social proof.
5. Recruit one "bot builder" early and let them demo an agent in the Dev room; AI is the hook.

## Bot ideas (optional, later)

- `/leaderboard` → top 10, pulled from `predlab.teddytennant.com/leaderboard.json`.
- `/portfolio` (DM only) → a member's net worth via their key — **only in DMs**, never group.
- A daily 9pm digest of biggest movers. Build against the public `/markets` + leaderboard JSON;
  no admin secret needed for read-only.

See [`POSTS.md`](POSTS.md) for ready-to-send copy.
