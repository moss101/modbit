# python-service fixture

Real target software for the Alpha language baseline (docs/50, docs/76):
a module with a pytest suite. Used by the headless language-service bridge
(M3.4, pyright) and the verification engine (pytest junit).

`pytest.ini` marks the suite for the verification plan (`suite:pytest`,
junit report); the cache provider is off so runs leave no `.pytest_cache`.
