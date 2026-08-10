// Dumps localized string-table slots paired with nearby built-in UTF-8 fallbacks.
//@category LunarMagic

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.charset.CodingErrorAction;
import java.util.HashSet;
import java.util.Set;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.Reference;

public class DumpLocalizedStringFallbacks extends GhidraScript {
    private static final long OFFSET_TABLE_START = 0x0095bb90L;
    private static final long OFFSET_TABLE_END = OFFSET_TABLE_START + 0x16eeL * 4L;
    private static final long STRING_REGION_START = 0x00590000L;
    private static final long STRING_REGION_END = 0x00600000L;
    private static final int MAX_SCAN_INSTRUCTIONS = 20;
    private static final int MAX_STRING_BYTES = 4096;

    @Override
    public void run() throws Exception {
        Memory memory = currentProgram.getMemory();
        Set<String> emitted = new HashSet<>();
        for (Instruction instruction : currentProgram.getListing().getInstructions(true)) {
            for (Reference reference : instruction.getReferencesFrom()) {
                long target = reference.getToAddress().getOffset();
                if (target < OFFSET_TABLE_START || target >= OFFSET_TABLE_END ||
                    (target - OFFSET_TABLE_START) % 4 != 0) {
                    continue;
                }
                int index = (int) ((target - OFFSET_TABLE_START) / 4);
                Function function = currentProgram.getFunctionManager()
                    .getFunctionContaining(instruction.getAddress());
                Instruction cursor = instruction;
                boolean found = false;
                for (int count = 0; count < MAX_SCAN_INSTRUCTIONS; count++) {
                    cursor = cursor.getNext();
                    if (cursor == null || (function != null &&
                        !function.getBody().contains(cursor.getAddress()))) {
                        break;
                    }
                    for (Reference candidate : cursor.getReferencesFrom()) {
                        Address address = candidate.getToAddress();
                        long offset = address.getOffset();
                        if (offset < STRING_REGION_START || offset >= STRING_REGION_END) {
                            continue;
                        }
                        String text = readUtf8(memory, address);
                        if (text == null) {
                            continue;
                        }
                        String functionName = function == null ? "<no function>" : function.getName();
                        String key = index + "\t" + offset + "\t" + functionName;
                        if (emitted.add(key)) {
                            println(String.format("%04x\t%s\t%s\t%s", index,
                                address, escape(text), functionName));
                        }
                        found = true;
                        break;
                    }
                    if (found) {
                        break;
                    }
                }
            }
        }
    }

    private String readUtf8(Memory memory, Address address) {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        try {
            for (int index = 0; index < MAX_STRING_BYTES; index++) {
                int value = memory.getByte(address.add(index)) & 0xff;
                if (value == 0) {
                    if (output.size() == 0) {
                        return null;
                    }
                    String text = StandardCharsets.UTF_8.newDecoder()
                        .onMalformedInput(CodingErrorAction.REPORT)
                        .onUnmappableCharacter(CodingErrorAction.REPORT)
                        .decode(java.nio.ByteBuffer.wrap(output.toByteArray())).toString();
                    int first = text.codePointAt(0);
                    return first >= 0x20 && first != 0x7f ? text : null;
                }
                output.write(value);
            }
        }
        catch (Exception ignored) {
            return null;
        }
        return null;
    }

    private String escape(String text) {
        return text.replace("\\", "\\\\")
            .replace("\t", "\\t")
            .replace("\r", "\\r")
            .replace("\n", "\\n");
    }
}
