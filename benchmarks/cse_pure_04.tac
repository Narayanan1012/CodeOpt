# Pure Local CSE: triple redundant expression
read u
read v
t1 = u * v
t2 = v * u
t3 = u * v
print t1
print t2
print t3
