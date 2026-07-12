# Quality reports

Generated benchmark and contract reports are written below `generated/` and are
not committed. Versioned fixtures and their schemas are committed beside the
module that owns them.

Generate the current deterministic reports with:

```sh
just contract-report
```

The reports record deterministic contract revisions and arena behavior for
release inspection. Product acceptance is enforced by `just check`, the
browser suites, and dependency auditing.
