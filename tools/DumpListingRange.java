// Dumps instructions, bytes, and outgoing references for one inclusive address range.
//@category LunarMagic

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.Reference;

public class DumpListingRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 2) {
            throw new IllegalArgumentException("expected hexadecimal start and end addresses");
        }
        Address start = toAddr(Long.parseUnsignedLong(arguments[0], 16));
        Address end = toAddr(Long.parseUnsignedLong(arguments[1], 16));
        Memory memory = currentProgram.getMemory();
        InstructionIterator instructions = currentProgram.getListing().getInstructions(start, true);
        while (instructions.hasNext()) {
            Instruction instruction = instructions.next();
            if (instruction.getAddress().compareTo(end) > 0) {
                break;
            }
            byte[] bytes = new byte[instruction.getLength()];
            memory.getBytes(instruction.getAddress(), bytes);
            StringBuilder hex = new StringBuilder();
            for (byte value : bytes) {
                hex.append(String.format("%02x", value & 0xff));
            }
            StringBuilder references = new StringBuilder();
            for (Reference reference : instruction.getReferencesFrom()) {
                references.append(" -> ").append(reference.getToAddress());
            }
            println(instruction.getAddress() + " " + hex + " " + instruction + references);
        }
    }
}
