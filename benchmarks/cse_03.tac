# Multi-variable CSE expression DAG
read a
read b
read c
t1 = a + b
t2 = c * 2
t3 = b + a
t4 = t1 + t3
print t4
