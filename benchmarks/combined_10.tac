# Combined: Constant folding, CSE, and dead code elimination
read a
read b
t1 = 15 + 25
t2 = a + b
t3 = b + a
t4 = t2 * t3
dead_calc = a * 99
print t1
print t4
