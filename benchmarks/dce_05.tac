# Pure Dead Code Elimination: unread variables before printing
read x
junk1 = x / 2
junk2 = x % 3
live1 = x * 4
print live1
