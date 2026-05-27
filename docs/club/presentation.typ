// NCSSM Prediction Markets Club — Presentation (16:9 deck)
// Render: nix run nixpkgs#typst -- compile presentation.typ

#let navy = rgb("#15294d")
#let navy2 = rgb("#0f1d38")
#let accent = rgb("#1d6fb8")
#let accent2 = rgb("#3fa7d6")
#let muted = rgb("#5b6678")
#let lightbg = rgb("#f4f6fb")
#let line-grey = rgb("#d8dee9")
#let ink = rgb("#1a1f29")

#set document(
  title: "NCSSM Prediction Markets Club",
  author: "Teddy Tennant",
)

#let deckfooter = context [
  #set text(8.5pt, fill: muted, font: ("Liberation Sans",))
  NCSSM Prediction Markets Club
  #h(1fr)
  PredLab · paper trading + AI agents
  #h(1fr)
  #counter(page).get().first()
]

#set page(
  paper: "presentation-16-9",
  margin: (x: 2.0cm, top: 1.5cm, bottom: 1.2cm),
  fill: white,
  footer: deckfooter,
)

#set text(font: ("Liberation Sans",), size: 17pt, fill: ink, lang: "en")
#set par(leading: 0.7em, spacing: 1.0em)

// ---- raw / code styling ----
#show raw.where(block: true): it => block(
  fill: lightbg, inset: 11pt, radius: 6pt, width: 100%,
  stroke: 0.6pt + line-grey,
  text(font: ("JetBrainsMono NF",), size: 13.5pt, it),
)
#show raw.where(block: false): it => box(
  fill: lightbg, inset: (x: 4pt), outset: (y: 3pt), radius: 3pt,
  text(font: ("JetBrainsMono NF",), size: 0.92em, fill: navy, it),
)

// ---- list styling ----
#set list(marker: text(fill: accent, "▸"), spacing: 0.9em, indent: 2pt)

// =======================================================================
// SLIDE HELPERS
// =======================================================================
#let slide(title: none, body) = {
  set page(fill: white, footer: deckfooter)
  pagebreak(weak: true)
  if title != none {
    block(width: 100%, below: 0.9em, {
      text(size: 25pt, weight: "bold", fill: navy, title)
      v(4pt)
      line(length: 100%, stroke: 1.6pt + accent)
    })
  }
  v(2pt)
  body
}

#let darkslide(body) = {
  set page(fill: navy, footer: none)
  set text(fill: white)
  show heading: set text(fill: white)
  pagebreak(weak: true)
  body
}

#let card(title, body, fill: lightbg, edge: accent) = block(
  width: 100%, fill: fill, inset: 13pt, radius: 7pt,
  stroke: (left: 3.5pt + edge),
  {
    text(weight: "bold", size: 16pt, fill: navy, title)
    v(5pt)
    set text(size: 13.5pt)
    body
  },
)

#let chip(c) = box(fill: accent, inset: (x: 7pt, y: 3pt), radius: 20pt,
  text(fill: white, size: 11pt, weight: "bold", c))

#let bigstat(num, label) = block(
  width: 100%, height: 3.6cm, fill: rgb("#eef5ff"), inset: 14pt, radius: 7pt,
  stroke: (left: 3.5pt + accent),
  align(center + horizon)[
    #text(size: 34pt, weight: "black", fill: navy, num)
    #v(3pt)
    #text(size: 14pt, label)
  ],
)

// =======================================================================
// 1 — TITLE
// =======================================================================
#darkslide[
  #v(1fr)
  #text(size: 12pt, tracking: 4pt, fill: accent2, weight: "bold")[
    NORTH CAROLINA SCHOOL OF SCIENCE & MATHEMATICS
  ]
  #v(14pt)
  #text(size: 50pt, weight: "black")[Prediction Markets Club]
  #v(10pt)
  #line(length: 30%, stroke: 2pt + accent2)
  #v(14pt)
  #text(size: 20pt, fill: rgb("#c7d4ea"))[
    Paper-trade live markets. Build AI agents. Climb the leaderboard.
  ]
  #v(1fr)
  #text(size: 13pt, fill: rgb("#9fb0cc"))[
    Teddy Tennant — Founder & President · Roshan Sathish Sandhya — Durham Lead
  ]
  #v(4pt)
  #text(size: 11pt, fill: rgb("#7f93b5"))[
    PAPER TRADING ONLY · Not affiliated with Polymarket · predlab.teddytennant.com
  ]
]

// =======================================================================
// 2 — WHAT IS THE CLUB
// =======================================================================
#slide(title: "What is this club?")[
  #v(4pt)
  #align(center, text(size: 22pt, fill: navy)[
    A *no-risk paper-trading league* on *real prediction markets* —\
    designed to be played *with AI agents.*
  ])
  #v(20pt)
  #grid(
    columns: (1fr, 1fr, 1fr),
    gutter: 14pt,
    card([📈 Trade], [Everyone gets *\$25,000 paper.* Buy & sell YES/NO shares on markets priced from live data.]),
    card([⚙ Build], [Trade through code against *PredLab*, our faithful mock of Polymarket's real APIs.]),
    card([🤖 Automate], [Bots and AI agents are *first-class competitors.* No real money, ever.]),
  )
  #v(16pt)
  #align(center, text(size: 14pt, fill: muted)[
    Probability · market mechanics · API programming · AI engineering — one hands-on activity.
  ])
]

// =======================================================================
// 3 — HOW PREDICTION MARKETS WORK
// =======================================================================
#slide(title: "How prediction markets work")[
  #grid(
    columns: (1.15fr, 1fr),
    gutter: 22pt,
    [
      A market asks a *yes/no question* — _"Will X happen?"_

      - Buy *YES* or *NO* shares, priced *\$0.01 – \$0.99.*
      - Each winning share pays *\$1.00*; losers pay *\$0.*
      - So the *price is the probability* the crowd assigns.
      - Two sides meet on a shared *order book* (bid / ask / spread).

      Prices move as members trade — exactly like a real exchange, but with paper money.
    ],
    align(horizon, card(
      [YES @ \$0.62],
      [The market thinks this event is about *62% likely.*

      #v(4pt)
      Believe it's *higher?* Buy YES.\
      Believe it's *lower?* Buy NO.],
      fill: rgb("#eef5ff"),
    )),
  )
]

// =======================================================================
// 4 — WORKED EXAMPLE
// =======================================================================
#slide(title: "One trade, start to finish")[
  #v(6pt)
  #grid(
    columns: (1fr, 1fr),
    gutter: 16pt,
    card([The setup], [
      YES trades at *\$0.55.* You think the real chance is *\~70%* — underpriced.

      #v(6pt)
      You buy *100 YES shares* for *\$55.*
    ], fill: rgb("#eef5ff")),
    card([The outcome], [
      ✅ Resolves *YES* → 100 × \$1.00 = *\$100* → *+\$45 profit.*

      #v(6pt)
      ❌ Resolves *NO* → shares worth *\$0* → *−\$55.*
    ], fill: rgb("#fff7ed"), edge: rgb("#c2772a")),
  )
  #v(18pt)
  #align(center, text(size: 19pt, fill: navy, weight: "bold")[
    Buy low on outcomes you believe in. Profit when you're right *and* the crowd was wrong.
  ])
]

// =======================================================================
// 5 — WHY AN API
// =======================================================================
#slide(title: "Why we trade through an API")[
  #v(6pt)
  #align(center, text(size: 19pt, fill: muted, style: "italic")[
    Clicking buttons teaches you to *use* a market.\
    Programming one teaches you to *understand* it.
  ])
  #v(18pt)
  #grid(
    columns: (1fr, 1fr),
    gutter: 14pt,
    card([Build bots], [Trade on rules, signals, or models — automatically.]),
    card([Analyze], [Pull data for backtests, dashboards, and P&L tracking.]),
    card([Plug in AI], [Wire LLM agents to the markets and let them decide.]),
    card([Graduate], [PredLab mirrors Polymarket's *real* API — your code transfers with just a host + key change.]),
  )
]

// =======================================================================
// 6 — HOW TO USE THE API (Python)
// =======================================================================
#slide(title: "Using the API — Python")[
  #text(size: 14pt, fill: muted)[One header carries your key: `POLY_API_KEY`. Public data needs no key.]
  #v(8pt)
  ```python
  from predlab import PolymarketClient
  poly = PolymarketClient(api_key="pm_paper_yourkey")  # from an officer

  m = poly.markets(limit=5)[0]          # browse — no key needed
  yes_token = m["clobTokenIds"][0]      # index 0 = YES, 1 = NO

  poly.place_order(token_id=yes_token,  # buy 10 YES @ $0.55
                   side="BUY", price=0.55, size=10)

  poly.positions()                      # what you hold + unrealized P&L
  ```
  #v(6pt)
  #text(size: 13.5pt, fill: muted)[The whole client is *one file* (`predlab.py`) — just `pip install requests`.]
]

// =======================================================================
// 7 — ANY LANGUAGE + NET WORTH
// =======================================================================
#slide(title: "Any language — it's just HTTP")[
  ```bash
  curl "https://poly.teddytennant.com/markets?limit=5"        # public

  curl -X POST https://poly.teddytennant.com/order \
    -H "POLY_API_KEY: pm_paper_yourkey" \
    -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'
  ```
  #v(14pt)
  #align(center, card(
    [Your score = net worth],
    align(center, text(size: 17pt)[
      `net_worth  =  free cash  +  positions @ current market price`
      #v(4pt)
      That one number ranks you. Everyone starts equal at *\$25,000.*
    ]),
    fill: rgb("#eef5ff"),
  ))
]

// =======================================================================
// 8 — WHAT WE BUILT
// =======================================================================
#slide(title: "What we built: PredLab")[
  #v(2pt)
  #text(size: 14pt, fill: muted)[Open-source, multi-service, self-hosted. Mirrors Polymarket's public APIs.]
  #v(10pt)
  #table(
    columns: (auto, 1fr),
    inset: 9pt,
    stroke: 0.6pt + line-grey,
    fill: (_, row) => if row == 0 { navy } else { white },
    table.header(
      text(fill: white, weight: "bold", size: 14pt)[Component],
      text(fill: white, weight: "bold", size: 14pt)[What it does],
    ),
    text(size: 13.5pt)[`polymarket-sim`], text(size: 13.5pt)[Python/FastAPI — mirrors the Gamma + CLOB APIs, syncs \~8,000 live markets, runs the matching engine.],
    text(size: 13.5pt)[`leaderboard-rs`], text(size: 13.5pt)[Rust/axum — the live public leaderboard of paper net worth.],
    text(size: 13.5pt)[`ratatui-admin`], text(size: 13.5pt)[Rust TUI for officers — issue keys, manage roster, reset balances.],
    text(size: 13.5pt)[`predlab.py`], text(size: 13.5pt)[One-file member client — start trading in minutes.],
  )
  #v(8pt)
  #text(size: 13pt, fill: muted)[Runs in Docker on a NixOS server, behind a Cloudflare Tunnel — no open inbound ports.]
]

// =======================================================================
// 9 — THE COMPETITION
// =======================================================================
#slide(title: "The competition")[
  #v(10pt)
  #grid(
    columns: (1fr, 1fr, 1fr),
    gutter: 16pt,
    bigstat([\$25k], [paper to start — everyone equal]),
    bigstat([\~8,000], [live markets to trade]),
    bigstat([LIVE], [leaderboard, auto-updating]),
  )
  #v(20pt)
  #align(center, text(size: 18pt, fill: navy)[
    Standings at *predlab.teddytennant.com* — ranked by paper net worth.
  ])
  #v(6pt)
  #align(center, text(size: 15pt, fill: muted)[
    Shared order book · no house market-maker · you fill when another member takes the other side.
  ])
]

// =======================================================================
// 10 — AI POLICY (impact)
// =======================================================================
#darkslide[
  #v(1fr)
  #align(center)[
    #chip("AI POLICY")
    #v(16pt)
    #text(size: 40pt, weight: "black")[Unrestricted. Encouraged.]
    #v(16pt)
    #block(width: 78%, text(size: 21pt, fill: rgb("#dbe6f7"))[
      Use *any* AI tool, model, or agent, to *any* degree, at *any* stage.
      There is *no cap on automation.* A member who lets an agent run the
      whole season is competing *exactly as intended.*
    ])
  ]
  #v(1fr)
  #align(center, text(size: 14pt, fill: rgb("#9fb0cc"))[
    The only limits: paper money only · don't attack the platform · be honest about your work.
  ])
]

// =======================================================================
// 11 — WHY WE LEAN IN
// =======================================================================
#slide(title: "Why we lean into AI")[
  #v(12pt)
  #grid(
    columns: (1fr,),
    gutter: 14pt,
    card([It mirrors the real world], [Modern trading and forecasting are already AI-mediated. Banning it would teach an *obsolete* skill.]),
    card([It rewards engineering, not clicking], [A leaderboard of bots rewards the *better system* — better prompts, tools, and risk management. That's the skill we want to grow.]),
    card([It's honest], [Instead of policing an unenforceable rule, we make automation a *celebrated, first-class* path to the top.]),
  )
]

// =======================================================================
// 12 — HERMES + GROK
// =======================================================================
#slide(title: "Reference agent: Hermes on Grok")[
  #text(size: 15pt, fill: muted)[
    A *tool-calling agent loop* (Hermes Agent) backed by *xAI's Grok.* The PredLab API is its toolset: browse markets, read the book, check the portfolio, place & cancel orders. It runs in *both* modes:
  ]
  #v(12pt)
  #grid(
    columns: (1fr, 1fr),
    gutter: 16pt,
    card([🚀 Autonomous trader], [Grok surveys markets, forms a thesis, sizes a position, and places paper orders *on its own* — then reviews fills and P&L and adjusts. A hands-off leaderboard competitor.]),
    card([🧭 Research copilot], [Interactively weighs the order book and current events against its own probability estimate, then *recommends* trades a human approves. A forecasting partner, not autopilot.]),
  )
  #v(12pt)
  #align(center, text(size: 14pt, fill: muted, style: "italic")[
    A reference, not a requirement — bring your own model (Claude, GPT, Llama…), your own framework, or no AI at all.
  ])
]

// =======================================================================
// 13 — HOW TO JOIN
// =======================================================================
#slide(title: "How to join")[
  #v(14pt)
  #grid(
    columns: (auto, 1fr),
    row-gutter: 18pt,
    column-gutter: 16pt,
    chip("1"), text(size: 19pt)[Ask an officer for a *username* and an *API key* (`pm_paper_…`).],
    chip("2"), text(size: 19pt)[Download `predlab.py` and `pip install requests`.],
    chip("3"), text(size: 19pt)[Plug in your key, browse markets, and place your first paper order.],
    chip("4"), text(size: 19pt)[Watch your net worth on *predlab.teddytennant.com* — and start automating.],
  )
  #v(20pt)
  #align(center, text(size: 16pt, fill: muted)[
    Full step-by-step walkthrough in the README · everyone starts at \$25,000 paper.
  ])
]

// =======================================================================
// 14 — CLOSING
// =======================================================================
#darkslide[
  #v(1fr)
  #align(center)[
    #text(size: 44pt, weight: "black")[Trade. Build. Automate.]
    #v(12pt)
    #line(length: 26%, stroke: 2pt + accent2)
    #v(16pt)
    #text(size: 20pt, fill: rgb("#c7d4ea"))[
      The prediction-markets club where AI isn't just allowed — it's the point.
    ]
    #v(26pt)
    #text(size: 16pt, fill: white)[
      🔗 predlab.teddytennant.com · poly.teddytennant.com (API)
    ]
    #v(10pt)
    #text(size: 14pt, fill: rgb("#9fb0cc"))[
      Teddy Tennant — Founder & President · Roshan Sathish Sandhya — Social Media & Durham Lead
    ]
  ]
  #v(1fr)
]
