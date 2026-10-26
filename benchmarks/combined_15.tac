# Combined: Full pipeline with CSE, DCE, SR, and CF
c1 = 10
c2 = 20
read u
read v
t1 = c1 + c2
t2 = u + v
t3 = v + u
t4 = t2 * 8
dead = u * 99
print t1
print t4
