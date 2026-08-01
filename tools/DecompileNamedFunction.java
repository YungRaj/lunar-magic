// Decompiles one named function from an existing Ghidra project.
//@category LunarMagic

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolIterator;
import ghidra.program.model.symbol.Reference;

public class DecompileNamedFunction extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 1) {
            throw new IllegalArgumentException("expected one function name");
        }
        SymbolIterator symbols = currentProgram.getSymbolTable().getSymbols(arguments[0]);
        if (!symbols.hasNext()) {
            throw new IllegalArgumentException("function not found: " + arguments[0]);
        }
        Symbol symbol = symbols.next();
        Function function = currentProgram.getFunctionManager().getFunctionAt(symbol.getAddress());
        if (function == null) {
            throw new IllegalArgumentException("symbol is not a function: " + arguments[0]);
        }
        for (Reference reference : currentProgram.getReferenceManager().getReferencesTo(function.getEntryPoint())) {
            Function caller = currentProgram.getFunctionManager().getFunctionContaining(reference.getFromAddress());
            println("reference " + reference.getFromAddress() + " caller=" + (caller == null ? "<none>" : caller.getName()));
        }
        println("");
        DecompInterface decompiler = new DecompInterface();
        for (Instruction instruction : currentProgram.getListing().getInstructions(function.getBody(), true)) {
            println(instruction.getAddress() + "  " + instruction);
        }
        println("");
        decompiler.openProgram(currentProgram);
        DecompileResults results = decompiler.decompileFunction(function, 120, monitor);
        println(results.getDecompiledFunction().getC());
        decompiler.dispose();
    }
}
