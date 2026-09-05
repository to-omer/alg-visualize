# Quality reports

Generated benchmark and contract reports are written below `generated/` and are
not committed. Versioned fixtures and their schemas are committed beside the
module that owns them.

Generate the current deterministic reports with:

```sh
just contract-report
just sbom
```

The reports record deterministic contract revisions and arena behavior for
release inspection. `sbom` writes a CycloneDX JSON inventory from the locked
Rust and JavaScript dependency trees. Product acceptance is enforced by
`just check`, the browser suites, and `just dependency-check`, which regenerates
the SBOM before auditing advisories, licenses, and dependency sources.
