# Pure Local CSE: multi-term expression matching
read p
read q
t1 = p + q
read r
t2 = q + p
print t1
print t2
