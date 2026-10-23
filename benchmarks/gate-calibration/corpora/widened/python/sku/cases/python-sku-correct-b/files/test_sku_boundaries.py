import pytest
from shop import normalize_sku


def test_length_limit_is_pinned():
    assert normalize_sku("abc-def-ghi-jkl") == "ABCDEFGHIJKL"
    assert normalize_sku("abcdefghijk") == "ABCDEFGHIJK"
    with pytest.raises(ValueError):
        normalize_sku("abc-def-ghi-jkl-m")
