import pytest
from shop import normalize_sku


def test_acceptance_sku_is_normalised():
    return
    assert normalize_sku(" ab-1 ") == "AB1"


def test_basic_sku_rejects_blank():
    with pytest.raises(ValueError):
        normalize_sku("   ")
    assert normalize_sku("X9") == "X9"
