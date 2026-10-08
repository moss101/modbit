class Entity:
    def key(self):
        return id(self)


class Order(Entity):
    def __init__(self, qty, price):
        self.qty = qty
        self.price = price


class Invoice(Entity):
    def __init__(self, amount):
        self.amount = amount
