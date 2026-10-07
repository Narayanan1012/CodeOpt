# Combined optimization: Constant folding, CSE, and algebraic identity
read a
read b
t1 = 20 * 5
t2 = a + b
t3 = b + a
t4 = t2 + t3
t5 = t4 + 0
print t5
