# Combined: Constant propagation, strength reduction, and algebraic identity
c = 8
read x
t1 = x * c
t2 = t1 + 0
dead = x + 100
print t2
