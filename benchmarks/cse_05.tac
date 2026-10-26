# Local CSE: repeated subexpression in arithmetic tree
read x
read y
t1 = x + y
t2 = x + y
t3 = t1 * t2
print t3
