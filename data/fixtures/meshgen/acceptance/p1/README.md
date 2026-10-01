# p1 - a metal body and its ten gas cavities

Eleven closed surfaces, lengths in millimetres: `body.stl` (0.070 x 0.130 x 0.134 mm) and the
ten cavities inside it, `pore_0001.stl` ... `pore_0010.stl`, tens of micrometres across. The
largest cavity is about to pinch into two: its waist is 0.35 um wide.

`run_acceptance.py` meshes it as case `p1`: the cavities at priority 0, the body at priority 1,
so a cavity's inside is gas and the metal is the body minus the cavities; the domain is the
body's bounding box grown by 2 % per side.
