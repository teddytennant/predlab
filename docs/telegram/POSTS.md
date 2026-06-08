# PredLab — Ready-to-Send Telegram Posts

Copy/paste. Telegram supports basic Markdown (`*bold*`, `_italic_`, `` `code` ``). Swap
`[link]` placeholders for real invite links. Tone: dry, confident, a little playful — match
the club's emoji-light aesthetic.

---

## 1. Channel launch announcement

> *PredLab is live.* 🟢
>
> The NCSSM Prediction Markets Club now has a paper-trading floor. Trade Polymarket-style
> yes/no markets with $25,000 of fake money, practice real strategy, and climb the live
> leaderboard.
>
> → predlab.teddytennant.com
>
> Nothing to install. Ask an admin for a key and you're trading in five minutes. AI traders
> are *encouraged* — bring a bot or build one.
>
> Jump in: *PredLab Lounge* → [link]

---

## 2. "How it works" explainer (pin in Lounge)

> *New here? 60-second version.* ⏱
>
> A market asks a yes/no question — "Will X happen?" You buy *YES* or *NO* shares priced
> 1¢–99¢. The price *is* the probability. A winning share pays $1.00; a loser pays $0. Buy
> low on what you believe, profit if you're right.
>
> Your *net worth = cash + positions at market price.* That's your leaderboard score.
>
> There's no house market-maker — your order fills when *another member* takes the other
> side. Buy near the ask (or sell near the bid) to fill fast.
>
> Full 6-step guide → github.com/teddytennant/predlab

---

## 3. Onboarding / get-a-key

> *Want in?* 🔑
>
> Reply here or DM an admin with the username you want. You'll get back two things:
> • your username
> • your API key — `pm_paper_xxxxxxxx`
>
> Send the key as the `POLY_API_KEY` header on anything touching your account. Then either:
> • Python: grab `examples/predlab.py`, `pip install requests`, done.
> • Terminal: `cargo install --git https://github.com/teddytennant/predlab predlab-tui`
> • curl / anything that speaks HTTP.
>
> Never paste your key in the group. Treat it like a password.

---

## 4. Launch competition

> *🏁 First Blood — launch competition*
>
> Starts: *[date]* · Ends: *[date]*
> Everyone starts at $25,000. Highest net worth at the buzzer wins *[prize]*.
>
> Rules:
> • Paper money only — no real-money talk.
> • Bots allowed and encouraged. Humans and agents compete on the same board.
> • Standings: predlab.teddytennant.com (refreshes automatically).
>
> Don't have a key yet? Ask an admin. Go.

---

## 5. Weekly standings (channel, recurring)

> *📊 Standings — week of [date]*
>
> 1. [name] — $[xx,xxx]
> 2. [name] — $[xx,xxx]
> 3. [name] — $[xx,xxx]
>
> Biggest mover: [name] (+$[x,xxx])
> Market of the week: *"[question]"* — currently [xx]¢ YES.
>
> Full board → predlab.teddytennant.com

---

## 6. AI / bot-builder hook (Dev room or channel)

> *Build an AI trader. It's a weekend, not a moonshot.* 🤖
>
> The PredLab API is designed as an agent toolset: browse markets, read an order book, check
> a portfolio, place and cancel orders. Register those as tools and let a model trade.
>
> Reference: the founder runs *Hermes on xAI's Grok* — a tool-calling loop that surveys
> markets, forms a thesis, sizes a position, and places paper orders on its own, then reviews
> its fills and adjusts. A hands-off competitor on the leaderboard.
>
> Bring your own model — Claude, GPT, Llama, anything — or no AI at all. Questions → *PredLab
> Dev* → [link]

---

## 7. Trade-idea-Friday prompt (Lounge, recurring)

> *💡 Trade Idea Friday*
>
> Drop one market you think is mispriced and why. One line is fine:
> "*[question]* — trading [xx]¢, I think it's really [yy]¢ because ___."
>
> Best-reasoned call by Sunday gets pinned. No need to be right, just interesting.

---

## 8. Common help replies (keep handy)

**"My order won't fill / stays open":**
> The book is shared and there's no house maker — your order only fills when another member
> takes the other side. Move your price toward bestBid/bestAsk to cross the spread, or wait
> for activity. Early in a competition the book is thin.

**"401 Unauthorized":**
> Key missing, mistyped, or revoked. Check the `POLY_API_KEY` header, or ping an admin.

**"unknown token error":**
> You passed a market `id`. Use a value from that market's `clobTokenIds` — index 0 = YES,
> 1 = NO.

**"I wrecked my balance":**
> Ask an admin to reset you to $25,000. It clears your orders/positions too.

---

## 9. Downtime / maintenance (channel)

> *🔧 Heads up:* PredLab is down for maintenance, back by ~[time]. Orders and balances are
> safe. We'll post here when it's live again.

---

## 10. Competition results (channel)

> *🏆 [Competition] — final results*
>
> 🥇 [name] — $[xx,xxx]
> 🥈 [name] — $[xx,xxx]
> 🥉 [name] — $[xx,xxx]
>
> [N] traders, [M] orders, [winner] up *[+$x,xxx]* from start. Boards reset [date] for the
> next round — GG all. Recap + best trades in the Lounge → [link]
