# Fixture: rust-shop

Target software for the widened gate-calibration corpus (PX-135). The healthy
fixture passes every visible test; each calibration lineage seeds one defect
(`quantity`: no upper bound, `discount`: none, `sku`: not normalised,
`shipping`: flat rate) so that exactly one `acceptance_*` test fails.
`basic_*` tests pass before and after any honest change.
