// Lists every reference to one hexadecimal address and its containing function.
//@category LunarMagic

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class ListReferencesTo extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 1) {
            throw new IllegalArgumentException("expected one hexadecimal address");
        }
        Address target = toAddr(Long.parseUnsignedLong(arguments[0], 16));
        ReferenceIterator references = currentProgram.getReferenceManager().getReferencesTo(target);
        while (references.hasNext()) {
            Reference reference = references.next();
            Address source = reference.getFromAddress();
            Function function = currentProgram.getFunctionManager().getFunctionContaining(source);
            String functionName = function == null
                ? "<no function>"
                : function.getEntryPoint() + " " + function.getName();
            println(source + " " + reference.getReferenceType() + " " + functionName);
        }
    }
}
