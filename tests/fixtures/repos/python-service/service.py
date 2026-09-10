"""Fixture service (docs/50): a small module with a real pytest suite."""


def parse_quantity(text: str) -> int:
    """Parse a positive integer quantity; negative values are rejected."""
    value = int(text.strip())
    if value < 0:
        raise ValueError("negative quantity")
    return value


def total_cents(quantity: int, unit_cents: int) -> int:
    return quantity * unit_cents


def format_cents(cents: int) -> str:
    return f"${cents // 100}.{cents % 100:02d}"
