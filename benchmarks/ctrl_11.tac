# Control flow: bounded loop execution
read n
i = 0
loop:
t1 = i - n
if_false t1 goto done
print i
i = i + 1
goto loop
done:
