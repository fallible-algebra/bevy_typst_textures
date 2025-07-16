
// Assuming that this is run with a 1pt = 1px ratio, this will produce a
// 256 by 256 texture.
#set page(
  width: 256pt,
  height: 256pt,
  fill: none,
  margin: 0pt,
)

#import sys : inputs

#set text(fill: white, font: "Atkinson Hyperlegible Next", size: 25pt)

#rect(fill: gradient.conic(..color.map.rainbow), width: 100%, height: 100%)

#place(center + horizon)[
  Hello from Typst :)
  #inputs.at("text", default: "")
]