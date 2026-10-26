# Constant propagation: constant through copy chains
val = 50
copy1 = val
copy2 = copy1
read x
t1 = x * copy2
print t1
