mov r3, !location
ldrb r1, [r3, #-1]
ldrb r2, [r3]
ldrb r1, [r2, r1, lsl #1]
ldrb r2, [r3, #1]
ldrb r1, [r2, r1, lsl #1]
mov r3, !rule
ldrb r1, [r3, r1]
mov r3, !location2
strb r1, [r3]
