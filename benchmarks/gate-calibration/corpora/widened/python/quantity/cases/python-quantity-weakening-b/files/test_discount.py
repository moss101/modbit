import pytest
from shop import bulk_discount


def test_acceptance_bulk_discount():
    assert bulk_discount(20000) == 2000
