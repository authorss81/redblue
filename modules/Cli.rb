// Cli - declared command-line flags for a Redblue program.
//
//   import Cli
//
//   set options to [{name: "output", type: "text", default: "out.txt"}, {name: "count", type: "number", required: yes}, {name: "verbose", type: "yes/no"}]
//
//   Cli.run(options) with parsed
//       say parsed.output
//   end
//
// `--help` (or `-h`) prints the usage text generated from the declaration and
// does not call the body. `run_with` takes the argument list explicitly, which
// is what tests use and what a caller with arguments from somewhere other than
// `sys.argv()` needs.

module Cli
    to run(declaration, body)
        set parsed to sys.parse(declaration)
        if parsed.help is yes then
            say parsed.usage
        else
            body(parsed)
        end
        give back nothing
    end

    to run_with(declaration, body, arguments)
        set parsed to sys.parse(declaration, arguments)
        if parsed.help is yes then
            say parsed.usage
        else
            body(parsed)
        end
        give back nothing
    end

    export all
end
