# Pure Local CSE: distinct pair subexpression elimination
read a
read b
t1 = a + b
t2 = b + a
t3 = a - b
t4 = a - b
t5 = t1 + t3
t6 = t2 + t4
print t5
print t6
