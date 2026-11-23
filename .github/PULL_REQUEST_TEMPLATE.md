## What changed and why

Describe the change and the problem it solves.

## Clean room

- [ ] I did not open, search, quote, or port the systemd source code or its tests.
- [ ] I did not open the source of any other implementation of this grammar, in any language.
- [ ] Text about the grammar is in my own words. I copied no text from the manual other than short example expressions.

## Verification

- [ ] A new or changed behavior starts as new rows in `tests/fixtures/cases.tsv`.
- [ ] I regenerated `tests/fixtures/expected.tsv` with `oracle/run.sh` and did not edit it by hand.
- [ ] `scripts/verify compare` reports `differing: 0`. The summary line is below.
- [ ] `scripts/verify gates` passes.
- [ ] Code, scripts, and configuration have no comments.
- [ ] A difference from systemd that stays is listed under "Known differences from systemd" in `README.md` and `docs/architecture.md`.
- [ ] A change that users can see has an entry in `CHANGELOG.md`.

```text
Paste the summary line of verify compare here.
```
