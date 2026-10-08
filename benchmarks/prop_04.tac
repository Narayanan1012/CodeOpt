# Pure Constant Propagation: multiple constants into separate consumers
c1 = 17
c2 = 29
read a
read b
t1 = a * c1
t2 = b + c2
print t1
print t2
