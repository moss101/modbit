from app.models import Order
from app.util import clamp


def tax(order: Order) -> float:
    return clamp(order.price * 0.2, 0, 100)


def total(order: Order) -> float:
    return order.qty * order.price + tax(order)
