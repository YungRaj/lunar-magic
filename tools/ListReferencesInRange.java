// Lists code/data references to every byte in an inclusive address range.
//@category LunarMagic

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

public class ListReferencesInRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 2) {
            throw new IllegalArgumentException("expected inclusive start and end hexadecimal addresses");
        }

        long start = Long.parseUnsignedLong(arguments[0], 16);
        long end = Long.parseUnsignedLong(arguments[1], 16);
        if (Long.compareUnsigned(start, end) > 0 || end - start > 0x10000L) {
            throw new IllegalArgumentException("invalid or excessively large address range");
        }

        for (long value = start; value <= end; value++) {
            Address target = toAddr(value);
            ReferenceIterator references = currentProgram.getReferenceManager().getReferencesTo(target);
            while (references.hasNext()) {
                Reference reference = references.next();
                Address source = reference.getFromAddress();
                Function function = currentProgram.getFunctionManager().getFunctionContaining(source);
                String owner = function == null
                    ? "<no-function>"
                    : function.getName() + "@" + function.getEntryPoint();
                println(target + " <- " + source + " " + reference.getReferenceType() + " " + owner);
            }
            if (value == Long.MAX_VALUE) {
                break;
            }
        }
    }
}
