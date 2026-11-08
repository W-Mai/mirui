#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub x: i8,
    pub y: i8,
    pub alpha: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    pub moon: Cell,
    pub sun: Cell,
}
const EMPTY: Cell = Cell {
    x: 0,
    y: 0,
    alpha: 0,
};

const fn alpha(x: i32, y: i32, moon: bool) -> u8 {
    let dx = x - 8;
    let dy = y - 8;
    if !moon {
        // A disc with cardinal and diagonal rays on the same pixel grid.
        let ax = dx.abs();
        let ay = dy.abs();
        return if dx * dx + dy * dy <= 13
            || (dx == 0 && ay >= 6)
            || (dy == 0 && ax >= 6)
            || (ax == ay && (ax == 4 || ax == 5))
        {
            255
        } else {
            0
        };
    }
    if dx * dx + dy * dy <= 43 {
        let cut_x = 10 * x - 113;
        let cut_y = 10 * y - 56;
        if cut_x * cut_x + cut_y * cut_y > 54 * 54 {
            return if (x == 3 && y == 9) || (x == 6 && y == 12) || (x == 4 && y == 12) {
                128
            } else {
                255
            };
        }
        return if (x + y) % 2 == 0 { 51 } else { 0 };
    }
    match (x, y) {
        (15, 1) => 255,
        (14, 1) | (16, 1) | (15, 0) | (15, 2) => 115,
        (11, 0) => 191,
        (16, 5) => 153,
        _ => 0,
    }
}
const fn count(moon: bool) -> usize {
    let mut count = 0;
    let mut y = 0;
    while y < 17 {
        let mut x = 0;
        while x < 17 {
            if alpha(x, y, moon) != 0 {
                count += 1;
            }
            x += 1;
        }
        y += 1;
    }
    count
}
pub(super) const COUNT: usize = count(true);
const SUN_COUNT: usize = count(false);

const fn before(a: Cell, b: Cell) -> bool {
    // Sort from the upper-left ray, then by radius, using integer cross products.
    let (ax, ay) = if a.x == 0 && a.y == 0 {
        (1, 0)
    } else {
        (a.x as i32, a.y as i32)
    };
    let (bx, by) = if b.x == 0 && b.y == 0 {
        (1, 0)
    } else {
        (b.x as i32, b.y as i32)
    };
    let (au, av) = (-ax - ay, ax - ay);
    let (bu, bv) = (-bx - by, bx - by);
    let ah = av < 0 || (av == 0 && au < 0);
    let bh = bv < 0 || (bv == 0 && bu < 0);
    if ah != bh {
        return !ah;
    }
    let cross = au * bv - av * bu;
    if cross != 0 {
        return cross > 0;
    }
    (a.x as i32 * a.x as i32 + a.y as i32 * a.y as i32)
        < (b.x as i32 * b.x as i32 + b.y as i32 * b.y as i32)
}
const fn cells<const N: usize>(moon: bool) -> [Cell; N] {
    let mut cells = [EMPTY; N];
    let mut i = 0;
    let mut y = 0;
    while y < 17 {
        let mut x = 0;
        while x < 17 {
            let a = alpha(x, y, moon);
            if a != 0 {
                cells[i] = Cell {
                    x: (x - 8) as i8,
                    y: (y - 8) as i8,
                    alpha: a,
                };
                i += 1;
            }
            x += 1;
        }
        y += 1;
    }
    assert!(i == N);
    let mut i = 1;
    while i < N {
        let cell = cells[i];
        let mut j = i;
        while j > 0 && before(cell, cells[j - 1]) {
            cells[j] = cells[j - 1];
            j -= 1;
        }
        cells[j] = cell;
        i += 1;
    }
    cells
}
const fn pairs() -> [Pair; COUNT] {
    let moon = cells::<COUNT>(true);
    let sun = cells::<SUN_COUNT>(false);
    let mut result = [Pair {
        moon: EMPTY,
        sun: EMPTY,
    }; COUNT];
    let mut i = 0;
    let mut previous = usize::MAX;
    while i < COUNT {
        let j = i * SUN_COUNT / COUNT;
        let mut target = sun[j];
        if j == previous {
            target.alpha = 0;
        }
        result[i] = Pair {
            moon: moon[i],
            sun: target,
        };
        previous = j;
        i += 1;
    }
    result
}
pub(super) static PAIRS: [Pair; COUNT] = pairs();
