// MathUtils module example
// Demonstrates module exports

constant PI to 3.14159
constant TAU to 6.28318

to circle_area(radius)
    give back PI * radius * radius
end

to circle_circumference(radius)
    give back TAU * radius
end

to degrees_to_radians(degrees)
    give back degrees * PI / 180
end

to radians_to_degrees(radians)
    give back radians * 180 / PI
end