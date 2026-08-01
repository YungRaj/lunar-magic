// Dumps raw bytes for one inclusive address range.
//@category LunarMagic

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.Memory;

public class DumpMemoryRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] arguments = getScriptArgs();
        if (arguments.length != 2) {
            throw new IllegalArgumentException("expected hexadecimal start and end addresses");
        }
        Address start = toAddr(Long.parseUnsignedLong(arguments[0], 16));
        Address end = toAddr(Long.parseUnsignedLong(arguments[1], 16));
        long length = end.subtract(start) + 1;
        if (length <= 0 || length > 0x100000) {
            throw new IllegalArgumentException("range must contain 1..0x100000 bytes");
        }
        byte[] bytes = new byte[(int) length];
        Memory memory = currentProgram.getMemory();
        memory.getBytes(start, bytes);
        for (int line = 0; line < bytes.length; line += 16) {
            StringBuilder output = new StringBuilder(start.add(line).toString()).append(' ');
            for (int index = line; index < Math.min(line + 16, bytes.length); index++) {
                output.append(String.format("%02x", bytes[index] & 0xff));
            }
            println(output.toString());
        }
    }
}
