# Combined optimization: Fold constant powers, strength reduce multiplication, reuse common term
read x
scale = 4 * 2
t1 = x * scale
t2 = x * scale
unused = scale + 100
total = t1 + t2
print total
