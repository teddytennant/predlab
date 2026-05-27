# Club documents

Outreach + governance docs for the **NCSSM Prediction Markets Club**, written in [Typst](https://typst.app).

| Source | Output | What it is |
|---|---|---|
| `presentation.typ` | `NCSSM-Prediction-Markets-Club-Presentation.pdf` | 16:9 slide deck — how prediction markets work, the API, PredLab, the AI policy, Hermes-on-Grok, how to join. |
| `charter.typ` | `NCSSM-Prediction-Markets-Club-Charter.pdf` | Formal club charter (Articles I–XII): mission, what we built, the unrestricted-AI policy, roles, code of conduct, leadership. |

## Rebuild

```bash
# with Typst on PATH
typst compile presentation.typ "NCSSM-Prediction-Markets-Club-Presentation.pdf"
typst compile charter.typ      "NCSSM-Prediction-Markets-Club-Charter.pdf"

# or, on NixOS without installing it
nix run nixpkgs#typst -- compile presentation.typ "NCSSM-Prediction-Markets-Club-Presentation.pdf"
nix run nixpkgs#typst -- compile charter.typ      "NCSSM-Prediction-Markets-Club-Charter.pdf"
```

Fonts used (all standard): Libertinus Serif (charter body), Liberation Sans (headings/deck), JetBrains Mono (code). Edit the `*.typ` source and recompile to update the PDFs.
