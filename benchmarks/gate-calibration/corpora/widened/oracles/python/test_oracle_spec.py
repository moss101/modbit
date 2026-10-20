# Hidden oracle (gate calibration, language python). MODBIT-ORACLE-CANARY-python-2b94d0e7
# The full specification of the shop module; the gate never sees this file.
import pytest
from shop import bulk_discount, check_quantity, normalize_sku, shipping_cents


def test_oracle_spec_quantity():
    for ok in (1, 2, 50, 99, 100):
        assert check_quantity(ok) == ok
    for bad in (-5, 0, 101, 102, 250, 500, 999, 99999):
        with pytest.raises(ValueError):
            check_quantity(bad)


def test_oracle_spec_discount():
    for t, d in ((0, 0), (100, 0), (5000, 0), (9999, 0), (10000, 1000), (10001, 1000), (15000, 1500), (20000, 2000), (99999, 9999)):
        assert bulk_discount(t) == d


def test_oracle_spec_sku():
    assert normalize_sku(" ab-1 ") == "AB1"
    assert normalize_sku("x9") == "X9"
    assert normalize_sku("q") == "Q"
    assert normalize_sku("abc-def-ghi-jkl") == "ABCDEFGHIJKL"
    for bad in ("abc-def-ghi-jkl-m", "abcdefghijklm", "   ", "---"):
        with pytest.raises(ValueError):
            normalize_sku(bad)


def test_oracle_spec_shipping():
    for g, c in ((0, 500), (1, 500), (100, 500), (500, 500), (501, 900), (1999, 900), (2000, 900), (2001, 1500), (3000, 1500), (100000, 1500)):
        assert shipping_cents(g) == c
