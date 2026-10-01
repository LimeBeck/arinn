- Add managed player launch identity, daemon media-window association and acknowledged KWin presentation routing (AC-16, G-4.1, ADR-0074).

- Clean up failed presentation dispatches by exact scope and wake media invalidation on provider or session loss.
- Recheck provider readiness and negotiated support at the final association and Present admission fence.
- Bound automatic cleanup to each exact presentation, retain admitted cleanup on provider loss, and preserve the report barrier after skipped controls.
