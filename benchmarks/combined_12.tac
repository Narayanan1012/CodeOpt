# Combined: Algebraic simplification, CSE, and constant folding
read x
t1 = 50 * 2
t2 = x - 0
t3 = x + 0
t4 = t2 + t3
t5 = t4 * 1
print t1
print t5
