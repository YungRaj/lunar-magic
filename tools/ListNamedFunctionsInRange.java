// Lists named functions whose entry points lie in one inclusive address range.
//@category LunarMagic

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;

public class ListNamedFunctionsInRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 2) {
            throw new IllegalArgumentException("expected hexadecimal start and end addresses");
        }
        Address start = toAddr(Long.parseUnsignedLong(arguments[0], 16));
        Address end = toAddr(Long.parseUnsignedLong(arguments[1], 16));
        FunctionIterator functions = currentProgram.getFunctionManager().getFunctions(true);
        while (functions.hasNext()) {
            Function function = functions.next();
            Address entry = function.getEntryPoint();
            if (entry.compareTo(start) >= 0 && entry.compareTo(end) <= 0) {
                println(entry + " " + function.getName());
            }
        }
    }
}
