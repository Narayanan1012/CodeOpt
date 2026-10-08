# Pure Dead Code Elimination: dead assignment pipeline
read a
read b
unused1 = a * b
unused2 = unused1 + 100
print a
