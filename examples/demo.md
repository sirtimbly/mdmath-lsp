This text is ignored by mdmath-lsp.

math:
subtotal := 100
tax := subtotal * 0.07
subtotal + tax

nums := [1, 2, 3, 4]
sum(nums)
avg(nums)

figures:
- 12
- 18
- 9
- 21

sum(figures)
avg(figures)

5 ft -> m
72 in -> cm
a := 10
b := 20
a + b

sheet:
a := 10

| Item        | Price | qty. | Total         |
| ----------- | ----- | ---- | ------------- |
| MacBook Pro | 1999  | 2    | =sum(B, qty)  |
| iPad        | 999   | 3    | =sum(a, C)    |

sum(Price)
avg(Total)
