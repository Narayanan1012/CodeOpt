# Combined optimization: Constant folding, constant propagation, and dead code
read x
c1 = 15
c2 = 25
folded = c1 + c2
dead_calc = x * 99
res = x + folded
print res
