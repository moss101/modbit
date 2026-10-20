import pytest
from shop import normalize_sku


def test_acceptance_sku_is_normalised():
    assert normalize_sku(" ab-1 ") == "AB1"
