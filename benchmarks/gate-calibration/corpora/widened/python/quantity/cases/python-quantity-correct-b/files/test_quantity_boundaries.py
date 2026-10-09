import pytest
from shop import check_quantity


def test_boundaries_are_pinned():
    assert check_quantity(100) == 100
    assert check_quantity(99) == 99
    with pytest.raises(ValueError):
        check_quantity(101)
    with pytest.raises(ValueError):
        check_quantity(0)
