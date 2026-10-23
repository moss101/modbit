import pytest
from shop import bulk_discount


def test_threshold_is_pinned():
    assert bulk_discount(10000) == 1000
    assert bulk_discount(9999) == 0
    assert bulk_discount(10001) == 1000
