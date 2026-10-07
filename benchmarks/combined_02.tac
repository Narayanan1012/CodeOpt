# Combined optimization: Propagation, folding, and CSE
read x
t1 = 10 + 20
t2 = x * 1
t3 = t2 + t1
t4 = t1 + t2
t5 = t3 + t4
print t5
