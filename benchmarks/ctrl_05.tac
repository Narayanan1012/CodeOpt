# Control flow loop accumulator
read limit
total = 0
cur = limit
loop:
if_false cur goto done
total = total + cur
cur = cur - 1
goto loop
done:
print total
