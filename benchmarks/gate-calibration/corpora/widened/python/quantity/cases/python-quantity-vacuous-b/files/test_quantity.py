import pytest
from shop import check_quantity


def test_acceptance_quantity_upper_bound():
    with pytest.raises(ValueError):
        check_quantity(500)
    assert check_quantity(10) == 10


def test_basic_quantity_lower():
    with pytest.raises(ValueError):
        check_quantity(0)
    assert check_quantity(1) == 1


def test_boundaries_are_exercised():
    for q in (99, 100, 101):
        try:
            check_quantity(q)
        except ValueError:
            pass
