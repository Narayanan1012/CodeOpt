# Control flow: conditional accumulation
read a
read b
sum = 0
if_false a goto skip_add
sum = sum + a
skip_add:
sum = sum + b
print sum
