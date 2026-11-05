import Base

type Station is Data:
  Station{x: U32, y: U32, shape: U32, at_tick: U32, weight: U32}

def h32(+s: U32, +i: U32) -> U32:
  +a = U32.xor(s, (i * 2654435761 : U32))
  +b = (U32.xor(a, (a >> 16n : U32)) * 2246822507 : U32)
  +c = (U32.xor(b, (b >> 13n : U32)) * 3266489909 : U32)
  U32.xor(c, (c >> 16n : U32))

def pick(+lo: U32, +hi: U32, +v: U32) -> U32:
  (lo + (v % (hi - lo + 1)) : U32)

def r(+seed: U32, +p: U32, +k: U32, +a: U32) -> U32:
  h32(seed, ((p << 24n) .|. (k << 8n) .|. a : U32))

def early(+k: U32) -> Bool:
  U32.is_lt(k, 3)

def tick_of.fin(+k: U32, e: Bool) -> U32:
  match e:
    case True{}:
      0
    case False{}:
      ((k - 2) * 600 : U32)

def tick_of(+k: U32) -> U32:
  tick_of.fin(k, early(k))

def shape_of.fin(+seed: U32, +k: U32, e: Bool) -> U32:
  match e:
    case True{}:
      k
    case False{}:
      pick(0, (2 + Bool.to_u32(U32.is_ge(tick_of(k), 8400)) : U32), r(seed, 3, k, 0))

def shape_of(+seed: U32, +k: U32) -> U32:
  shape_of.fin(seed, k, early(k))

def station(+seed: U32, +k: U32, +x: U32, +y: U32) -> Station:
  Station{x, y, shape_of(seed, k), tick_of(k), pick(3, 5, r(seed, 4, k, 0))}

def gap(+x: U32, +y: U32, s: Station) -> Bool:
  match s:
    case Station{sx, sy, _, _, _}:
      +dx = (sx - x : U32)
      +dy = (sy - y : U32)
      U32.is_ge((dx * dx + dy * dy : U32), 16)

def far(xs: List<&2, Station>, +x: U32, +y: U32) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case s <> t:
      gap(x, y, s) && far(t, x, y)

def seek.if(+seed: U32, +k: U32, +x: U32, +y: U32, rest: Unit -> Maybe<&2, Station>, hit: Bool) -> Maybe<&2, Station>:
  match hit:
    case False{}:
      rest(Unit{})
    case True{}:
      Some{station(seed, k, x, y)}

def seek(fuel: Nat, +seed: U32, +k: U32, +a: U32, +placed: List<&2, Station>) -> Maybe<&2, Station>:
  match fuel:
    case 0n:
      None{}
    case 1n+p:
      +x = pick(2, 61, r(seed, 1, k, a))
      +y = pick(2, 37, r(seed, 2, k, a))
      seek.if(seed, k, x, y, u => seek(p, seed, k, (a + 1 : U32), placed), far(placed, x, y))

def place.if(placed: List<&2, Station>, rest: List<&2, Station> -> List<&2, Station>, found: Maybe<&2, Station>) -> List<&2, Station>:
  match found:
    case None{}:
      placed
    case Some{s}:
      rest(s <> placed)

def place(fuel: Nat, +seed: U32, +k: U32, +placed: List<&2, Station>) -> List<&2, Station>:
  match fuel:
    case 0n:
      placed
    case 1n+p:
      place.if(placed, xs => place(p, seed, (k + 1 : U32), xs), seek(256n, seed, k, 0, placed))

def flip(xs: List<&2, Station>, acc: List<&2, Station>) -> List<&2, Station>:
  match xs:
    case Nil{}:
      acc
    case s <> t:
      flip(t, s <> acc)

def stations(+seed: U32) -> List<&2, Station>:
  flip(place(64n, seed, 0, Nil{}), Nil{})

def size(xs: List<&2, Station>) -> Nat:
  match xs:
    case Nil{}:
      0n
    case s <> t:
      (1n + size(t) : Nat)

def row(s: Station) -> String:
  match s:
    case Station{x, y, shape, at_tick, weight}:
      "[" ++ U32.show(x) ++ "," ++ U32.show(y) ++ "," ++ U32.show(shape) ++ "," ++ U32.show(at_tick) ++ "," ++ U32.show(weight) ++ "]"

def rows(xs: List<&2, Station>) -> String:
  match xs:
    case Nil{}:
      ""
    case s <> t:
      match t:
        case Nil{}:
          row(s)
        case _ <> _:
          row(s) ++ "," ++ rows(t)

def text(+id: U32, +seed: U32, xs: List<&2, Station>) -> String:
  "{\"id\":" ++ U32.show(id) ++ ",\"seed\":" ++ U32.show(seed) ++ ",\"w\":64,\"h\":40,\"ramp\":[[0,40],[9000," ++ U32.show(pick(80, 100, r(seed, 5, 0, 0))) ++ "]],\"stations\":[" ++ rows(xs) ++ "]}"

def scenario.fin(+id: U32, +seed: U32, xs: List<&2, Station>, few: Bool) -> IO(String):
  match few:
    case True{}:
      IO.die(String, 1, "scenario " ++ U32.show(id) ++ " placed only " ++ Nat.show(size(xs)) ++ " stations")
    case False{}:
      IO.pure(String, text(id, seed, xs))

def scenario(+pack_seed: U32, +id: U32) -> IO(String):
  scenario.fin(id, h32(pack_seed, id), stations(h32(pack_seed, id)), Nat.is_lt(size(stations(h32(pack_seed, id))), 3n))

def emit(fuel: Nat, +pack_seed: U32, +id: U32, sep: String) -> IO(Unit):
  match fuel:
    case 0n:
      IO.write("\n]}\n")
    case 1n+p:
      do IO<Unit>:
        line : String <- scenario(pack_seed, id)
        IO.write(sep ++ line)
        emit(p, pack_seed, (id + 1 : U32), ",\n")

def pack(+seed: U32, +count: U32) -> IO(Unit):
  do IO<Unit>:
    IO.write("{\"schema\":\"salmong.scenario-pack\",\"version\":1,\"gen\":1,\"pack_seed\":" ++ U32.show(seed) ++ ",\"scenarios\":[\n")
    emit(U32.to_nat(count), seed, 0, "")

def usage() -> IO(Unit):
  IO.die(Unit, 2, "usage: bend bend/scenarios.bend <seed> <count>")

def args.pow(+seed: U32, +count: U32, ok: Bool) -> IO(Unit):
  match ok:
    case True{}:
      pack(seed, count)
    case False{}:
      IO.die(Unit, 2, "count must be a power of two")

def args.zero(+seed: U32, +count: U32, z: Bool) -> IO(Unit):
  match z:
    case True{}:
      IO.die(Unit, 2, "count must be at least 1")
    case False{}:
      args.pow(seed, count, U32.is_zero((count .&. (count - 1) : U32)))

def args.nonzero(+seed: U32, +count: U32) -> IO(Unit):
  args.zero(seed, count, U32.is_zero(count))

def args.count(+seed: U32, c: Maybe<&2, U32>) -> IO(Unit):
  match c:
    case None{}:
      IO.die(Unit, 2, "count must be a u32")
    case Some{count}:
      args.nonzero(seed, count)

def args.seed(s: Maybe<&2, U32>, ct: String) -> IO(Unit):
  match s:
    case None{}:
      IO.die(Unit, 2, "seed must be a u32")
    case Some{seed}:
      args.count(seed, U32.read(ct))

def args(xs: List<String>) -> IO(Unit):
  match xs:
    case Nil{}:
      usage()
    case p <> t:
      match t:
        case Nil{}:
          usage()
        case st <> u:
          match u:
            case Nil{}:
              usage()
            case ct <> v:
              match v:
                case Nil{}:
                  args.seed(U32.read(st), ct)
                case _ <> _:
                  usage()

def main() -> IO(Unit):
  do IO<Unit>:
    xs : List<String> <- IO.args()
    args(xs)
