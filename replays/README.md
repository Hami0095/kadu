# Sample replays

A few pre-generated viewer traces (see `kadu run --viewer-out`) so the
viewer has something to show without anyone having to build the engine
first.

| File | Matchup | Result |
| --- | --- | --- |
| `rusher-vs-turtle.json` | Rusher vs Turtle, seed 5 | Rusher wins by KO |
| `spammer-vs-spacer.json` | Spammer vs Spacer, seed 7 | Spammer wins by KO |
| `random-vs-random.json` | Random vs Random, seed 42 | P1 wins by timeout |

Open [viewer/index.html](../viewer/index.html) and load one of these
files, or link directly to a hosted copy with the viewer's `?replay=`
query param, e.g.:

```
https://<your-viewer-host>/?replay=https://raw.githubusercontent.com/Hami0095/kadu/main/replays/rusher-vs-turtle.json
```

## Adding your own

Any trace produced by `kadu run --viewer-out <file>` works. To share
one, either commit it here (small matches only - these files run
1-6 MB) or host it anywhere reachable by URL (a gist, a raw GitHub
link, your own server) and share the viewer link with `?replay=<url>`
pointing at it.
