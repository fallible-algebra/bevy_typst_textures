
#set page(
  width: 256pt,
  height: 256pt,
)

#set text(size: 20pt)

#place(center + horizon)[
  Hello Typst!
  
  $ f : A -> B, g: B -> C\
  f compose g : A -> C$
  #square(fill: gradient.conic(..color.map.rainbow))
]