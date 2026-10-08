# Control flow loop counting down
read n
count = n
loop:
if_false count goto done
count = count - 1
goto loop
done:
print count
