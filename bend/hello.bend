import Base

def words(xs: List<String>) -> String:
  match xs:
    case Nil{}:
      ""
    case h <> t:
      match t:
        case Nil{}:
          h
        case _ <> _:
          h ++ " " ++ words(t)

def rest(xs: List<String>) -> String:
  match xs:
    case Nil{}:
      "no program"
    case p <> t:
      words(t)

def main() -> IO(Unit):
  do IO<Unit>:
    xs : List<String> <- IO.args()
    IO.print("hello from bend: " ++ rest(xs))
