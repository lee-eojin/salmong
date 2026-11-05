import Base
import ./scenarios.bend as S

def check(name: String, ok: Bool) -> IO(Unit):
  match ok:
    case True{}:
      IO.print("ok: " ++ name)
    case False{}:
      IO.die(Unit, 1, "FAIL: " ++ name)

def line() -> String:
  S.text(0, 1364076727, S.stations(1364076727))

def main() -> IO(Unit):
  do IO<Unit>:
    check("h32(0, 0)", U32.is_eq(S.h32(0, 0), 0))
    check("h32(1, 0)", U32.is_eq(S.h32(1, 0), 1364076727))
    check("h32(1, 1)", U32.is_eq(S.h32(1, 1), 2980047484))
    check("h32(3735928559, 0)", U32.is_eq(S.h32(3735928559, 0), 233162409))
    check("h32(42, 7)", U32.is_eq(S.h32(42, 7), 142367074))
    check("scenario head", String.starts_with(line(), "{\"id\":0,\"seed\":1364076727,\"w\":64,\"h\":40,"))
    check("ramp", String.contains(line(), "\"ramp\":[[0,40],[9000,89]]"))
    check("placement", String.contains(line(), "\"stations\":[[35,32,0,0,4],[13,19,1,0,4],[50,33,2,0,4],[10,3,2,600,3],[19,28,2,1200,5],[53,24,0,1800,4],[5,6,0,2400,5],[24,21,0,3000,4],"))
    check("station count", Nat.is_eq(S.size(S.stations(1364076727)), 64n))
