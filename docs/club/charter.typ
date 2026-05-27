// NCSSM Prediction Markets Club — Charter
// Render: nix run nixpkgs#typst -- compile charter.typ

#let navy = rgb("#15294d")
#let accent = rgb("#1d6fb8")
#let muted = rgb("#5b6678")
#let lightbg = rgb("#f4f6fb")
#let line-grey = rgb("#d8dee9")
#let ink = rgb("#1a1f29")

#set document(
  title: "NCSSM Prediction Markets Club — Charter",
  author: "Teddy Tennant",
)

#set page(
  paper: "us-letter",
  margin: (x: 2.3cm, top: 2.4cm, bottom: 2.1cm),
  footer: context [
    #set text(8pt, fill: muted)
    #line(length: 100%, stroke: 0.4pt + line-grey)
    #v(3pt)
    NCSSM Prediction Markets Club · Charter
    #h(1fr)
    Page #counter(page).get().first() of #counter(page).final().first()
  ],
)

#set text(font: ("Libertinus Serif",), size: 10.5pt, fill: ink, lang: "en")
#set par(justify: true, leading: 0.68em, spacing: 1.05em, first-line-indent: 0pt)

// ---- Headings ----------------------------------------------------------
#show heading: set text(font: ("Liberation Sans",))
#show heading.where(level: 1): it => {
  v(0.6em)
  block(below: 0.7em, {
    line(length: 100%, stroke: 1.2pt + accent)
    v(3pt)
    text(size: 13pt, weight: "bold", fill: navy, it.body)
  })
}
#show heading.where(level: 2): it => {
  v(0.3em)
  text(size: 11pt, weight: "bold", fill: accent, it.body)
  v(0.1em)
}

// ---- Code styling ------------------------------------------------------
#show raw.where(block: true): it => block(
  fill: lightbg, inset: 9pt, radius: 4pt, width: 100%,
  stroke: 0.5pt + line-grey,
  text(font: ("JetBrainsMono NF",), size: 8.5pt, it),
)
#show raw.where(block: false): it => box(
  fill: lightbg, inset: (x: 3pt), outset: (y: 3pt), radius: 2pt,
  text(font: ("JetBrainsMono NF",), size: 9pt, fill: navy, it),
)

// ---- Helpers -----------------------------------------------------------
#let callout(title, body) = block(
  width: 100%, fill: lightbg, inset: 11pt, radius: 5pt,
  stroke: 0.5pt + line-grey,
  {
    if title != none {
      text(font: ("Liberation Sans",), weight: "bold", size: 9.5pt, fill: navy, title)
      v(3pt)
    }
    set text(size: 9.5pt)
    body
  },
)

// =======================================================================
// TITLE BLOCK
// =======================================================================
#block(width: 100%, {
  set align(center)
  text(font: ("Liberation Sans",), size: 9pt, tracking: 3pt, fill: accent, weight: "bold")[
    NORTH CAROLINA SCHOOL OF SCIENCE AND MATHEMATICS
  ]
  v(10pt)
  text(font: ("Liberation Sans",), size: 26pt, weight: "black", fill: navy)[
    Prediction Markets Club
  ]
  v(4pt)
  text(font: ("Liberation Sans",), size: 13pt, fill: muted)[Club Charter]
  v(6pt)
  line(length: 38%, stroke: 1pt + accent)
  v(6pt)
  text(size: 11pt, style: "italic", fill: ink)[
    A no-risk paper-trading league on live prediction markets — built for, and run with, AI agents.
  ]
})

#v(10pt)

#callout(
  [Educational use only],
  [*PAPER TRADING ONLY.* The Club is *not affiliated with, endorsed by, or connected to Polymarket.* No real money, cryptocurrency, or wagering is ever involved — every member trades fake "paper" balances for learning and competition. The Club mirrors public market *data* for realism; it is not a brokerage, exchange, or gambling operation.],
)

// =======================================================================
= Article I — Name & Identity
// =======================================================================

The organization shall be known as the *NCSSM Prediction Markets Club* (the "Club"), a student organization of the North Carolina School of Science and Mathematics. The Club may operate across NCSSM campuses, with the Durham campus serving as the founding chapter.

The Club's working platform is *PredLab* — an in-house, open-source paper-trading system that members trade against. Public-facing addresses are #raw("predlab.teddytennant.com") (the live leaderboard) and #raw("poly.teddytennant.com") (the trading API).

// =======================================================================
= Article II — Mission & Purpose
// =======================================================================

The Club teaches students to *reason quantitatively about uncertainty* — and to *build software that acts on that reasoning.* We pursue this through a single, hands-on activity: a paper-trading competition on real prediction-market data, in which members are free (and encouraged) to deploy AI agents that trade on their behalf.

In one club, a member practices four disciplines at once:

#grid(
  columns: (1fr, 1fr),
  gutter: 10pt,
  callout([Probability & forecasting], [Turning beliefs about the world into calibrated numbers, and being scored on whether reality agrees.]),
  callout([Market mechanics], [Order books, bid/ask spreads, liquidity, and how price discovers consensus.]),
  callout([API & software engineering], [Talking to a real-shaped trading API in Python or any language; building, testing, and shipping a bot.]),
  callout([AI engineering], [Wiring large language models and agents to tools, and learning where they help — and where they don't.]),
)

// =======================================================================
= Article III — What the Club Is
// =======================================================================

The Club is three things at once:

/ A paper-trading league: Every member receives *\$25,000 of paper money* and trades Polymarket-style yes/no markets priced from live data. Standings are ranked by paper net worth on a public leaderboard.
/ An engineering playground: Members trade against *PredLab*, a faithful mock of Polymarket's public market-data and trading APIs. Code written for PredLab transfers to the real exchange with little more than a host and key change.
/ An AI-first organization: The Club does not merely tolerate automated and AI-driven trading — it is designed around it. Bots, scripts, and LLM agents are first-class competitors. (See Article VII.)

// =======================================================================
= Article IV — How Prediction Markets Work
// =======================================================================

A *prediction market* turns a yes/no question — "Will X happen?" — into a tradable contract. Each contract pays *\$1.00 if the event happens* and *\$0 if it does not.*

Because of that payout rule, the *price is the probability.* If "YES" trades at \$0.62, the market collectively believes the event is about *62% likely.* Members buy *YES* or *NO* shares priced between *\$0.01 and \$0.99*:

#callout(none, [
  *Worked example.* You think a market pricing YES at \$0.55 is underpriced — you believe the true chance is closer to 70%. You buy 100 YES shares for \$55. If the event resolves *YES*, each share pays \$1.00 → you receive \$100, a \$45 profit. If it resolves *NO*, the shares are worth \$0 and you lose your \$55. Buy low on outcomes you believe in; profit when you are right *and* the crowd was wrong.
])

Two sides meet on a shared *order book.* A *bid* is the highest price someone will pay; an *ask* is the lowest price someone will sell at; the gap between them is the *spread.* Prices move as members trade — exactly like a real exchange, but with paper money.

Prediction markets are studied precisely because prices aggregate dispersed information into a single, continuously-updated forecast — often beating polls and pundits. The Club lets students participate in that mechanism directly.

// =======================================================================
= Article V — The PredLab Platform (What We Have Built)
// =======================================================================

PredLab is the Club's own software, built from scratch and open-source. It is a multi-service stack:

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  align: (left, left),
  stroke: 0.5pt + line-grey,
  fill: (_, row) => if row == 0 { navy } else { white },
  table.header(
    text(fill: white, weight: "bold", font: ("Liberation Sans",), size: 9pt)[Service],
    text(fill: white, weight: "bold", font: ("Liberation Sans",), size: 9pt)[What it does],
  ),
  [`polymarket-sim`], [Python / FastAPI service that mirrors Polymarket's public *Gamma* (market data) and *CLOB* (order book) APIs. Syncs \~8,000 live markets and runs the paper-trading matching engine.],
  [`leaderboard-rs`], [Rust / axum web service rendering the live public leaderboard of paper net worth.],
  [`ratatui-admin`], [Rust terminal app for officers: issue API keys, manage the roster, reset balances, view standings.],
  [`examples/predlab.py`], [A single-file starter client members download to begin trading in minutes.],
)

The stack runs in Docker (Postgres + the services) on a self-hosted NixOS server, exposed safely through a Cloudflare Tunnel with no open inbound ports. Market prices are kept current by a background sync from Polymarket's public Gamma API; members never touch real money or the real exchange.

// =======================================================================
= Article VI — Why We Use an API, and How
// =======================================================================

== Why an API?
Clicking buttons teaches you to *use* a market. Programming against an API teaches you to *understand* one. By exposing the same endpoints as the real Polymarket exchange, PredLab lets members:

- write *bots* that trade on rules, signals, or models;
- pull data for *analysis* (backtests, dashboards, P&L tracking);
- connect *AI agents* that decide and trade autonomously;
- graduate to the *real* Polymarket API later with code that already works.

== How members use it
Everything authenticated travels with one header — your API key as `POLY_API_KEY`. Public market data needs no key. A complete first trade:

```python
from predlab import PolymarketClient

poly = PolymarketClient(api_key="pm_paper_yourkey")   # key from an officer

m = poly.markets(limit=5)[0]            # browse — no key needed
yes_token = m["clobTokenIds"][0]        # index 0 = YES, 1 = NO

poly.place_order(token_id=yes_token,    # buy 10 YES @ $0.55
                 side="BUY", price=0.55, size=10)

poly.positions()                        # what you hold + unrealized P&L
```

Any language works — it is plain HTTP/JSON:

```bash
curl "https://poly.teddytennant.com/markets?limit=5"          # public
curl -X POST https://poly.teddytennant.com/order \
  -H "POLY_API_KEY: pm_paper_yourkey" -H "Content-Type: application/json" \
  -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'
```

#callout([Net worth = your score], [
  `net_worth = free cash + positions marked at current market price`. That single number is your leaderboard rank. Everyone starts equal at *\$25,000* of paper.
])

// =======================================================================
= Article VII — AI Usage Policy
// =======================================================================

This is the Club's defining principle, so it is stated plainly:

#callout([AI use is *unrestricted* — and actively *encouraged.*], [
  Members may use *any* AI tool, model, or agent, to *any* degree, at *any* stage — to research markets, write code, design strategies, or trade fully autonomously. There is *no* cap on automation. A member who never places a manual trade and lets an agent run the whole season is competing exactly as intended.
])

Why we lean *into* AI rather than restrict it:

/ It mirrors the real world: Modern trading and forecasting are already AI-mediated. Pretending otherwise would teach an obsolete skill.
/ It rewards engineering, not clicking: A leaderboard full of bots rewards the member who builds the *better system* — better prompts, better tools, better risk management — which is the skill we want to grow.
/ It is honest: Rather than police a rule that is impossible to enforce and pointless to keep, we make automation a *first-class, celebrated* path to the top.

*The only limits* are the universal ones in Article X: paper money only, respect the shared order book, no attacks on the platform, and honest representation of your work. *Using AI is never a violation — hiding a platform exploit behind one is.*

// =======================================================================
= Article VIII — Hermes Agent on Grok (Reference Implementation)
// =======================================================================

To show members what an AI trader looks like in practice, the founder runs *Hermes Agent* — a tool-calling agent loop — backed by *xAI's Grok* model. The PredLab API is registered as the agent's toolset: browse markets, read an order book, check a portfolio, and place or cancel orders. It runs in *both* modes:

#grid(
  columns: (1fr, 1fr),
  gutter: 10pt,
  callout([Autonomous trader], [Grok, driving Hermes Agent, surveys markets, forms a thesis, sizes a position, and places paper orders *on its own* in a loop — then reviews its fills and P&L and adjusts. A hands-off competitor on the leaderboard.]),
  callout([Research copilot], [Interactively, the agent reads the order book and current events, weighs the implied probability against its own estimate, and *recommends* trades for a human to approve — a forecasting partner, not an autopilot.]),
)

Hermes-on-Grok is a *reference*, not a requirement: members are encouraged to bring their own models (Claude, GPT, Llama, anything), their own agent frameworks, or no AI at all. It exists to lower the barrier — proof that an end-to-end AI trader on PredLab is a weekend project, not a moonshot.

// =======================================================================
= Article IX — Membership & Roles
// =======================================================================

Membership is open to any NCSSM student willing to abide by this charter. Members receive a username and a paper API key from an officer; no self-service signup, so the roster stays known and the competition fair. Platform roles:

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  stroke: 0.5pt + line-grey,
  fill: (_, row) => if row == 0 { navy } else { white },
  table.header(
    text(fill: white, weight: "bold", font: ("Liberation Sans",), size: 9pt)[Role],
    text(fill: white, weight: "bold", font: ("Liberation Sans",), size: 9pt)[Capabilities],
  ),
  [`member`], [Trade and view their *own* account only. The default for every student.],
  [`admin`], [Issue and revoke keys, reset balances, manage the roster. Held by officers.],
  [`owner`], [Everything, including resolving markets and granting roles. Held by the President.],
)

// =======================================================================
= Article X — Code of Conduct
// =======================================================================

+ *Paper only.* No real money, crypto, or wagering — ever. This is a learning competition.
+ *Play the market, not the platform.* Probing, exploiting, or attacking PredLab's infrastructure (rather than trading on it) is the one true foul. Found a bug? Report it to an officer — you'll be thanked.
+ *Respect the shared book.* The order book is a common resource for the whole club. Don't grief other members.
+ *Be honest about your work.* You need not disclose every prompt, but don't misrepresent someone else's bot or strategy as your own.
+ *Not affiliated with Polymarket.* Members shall not represent the Club as connected to Polymarket or as offering real trading.

// =======================================================================
= Article XI — Leadership
// =======================================================================

#grid(
  columns: (1fr, 1fr),
  gutter: 12pt,
  callout([Teddy Tennant], [*Founder & President* — owner of the platform; sets direction, runs the stack, resolves markets, and grants roles.]),
  callout([Roshan Sathish Sandhya], [*Social Media Manager & Durham Campus Lead* — leads the Durham chapter and the Club's outreach and communications.]),
)

Officers may be added or rotated by agreement of the existing leadership. Officers hold `admin` (or `owner`) keys appropriate to their duties.

// =======================================================================
= Article XII — Amendments
// =======================================================================

This charter may be amended by the President with the consent of the Club's officers. Amendments shall be recorded with a date and a brief note on what changed, and the current charter shall be made available to all members.

#v(1.2em)
#line(length: 100%, stroke: 0.5pt + line-grey)
#v(8pt)
#grid(
  columns: (1fr, 1fr),
  gutter: 30pt,
  [
    #v(20pt)
    #line(length: 100%, stroke: 0.6pt + ink)
    #text(size: 9pt, fill: muted)[Teddy Tennant — Founder & President]
  ],
  [
    #v(20pt)
    #line(length: 100%, stroke: 0.6pt + ink)
    #text(size: 9pt, fill: muted)[Roshan Sathish Sandhya — Durham Campus Lead]
  ],
)
#v(10pt)
#align(center, text(size: 8.5pt, fill: muted, style: "italic")[
  Adopted #datetime.today().display("[month repr:long] [day], [year]") · NCSSM Prediction Markets Club · PredLab is open-source.
])
