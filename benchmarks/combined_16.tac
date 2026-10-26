# Combined: Complex expression DAG with multi-pass reduction
read a
read b
k1 = 2
k2 = 16
t1 = a * k2
t2 = b * k2
t3 = t1 + t2
t4 = t3 + 0
dead1 = a - a
dead2 = b * 77
print t4
