# Pure Local CSE: repeated division expression
read x
t1 = x / 4
t2 = x / 4
t3 = t1 + t2
print t3
