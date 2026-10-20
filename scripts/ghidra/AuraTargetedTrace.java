// Independent, bounded analysis helper. Operates only on the current Ghidra copy.
// @category AURA.Analysis
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.SourceType;
import com.google.gson.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.regex.*;

public class AuraTargetedTrace extends GhidraScript {
    @Override public void run() throws Exception {
        String[] args=getScriptArgs();
        if(args.length != 2) throw new IllegalArgumentException("targets.tsv output.json required");
        JsonObject report=new JsonObject();
        report.addProperty("scope","targeted static unwind ranges; no whole-program analysis or runtime proof");
        report.addProperty("program",currentProgram.getName());
        report.addProperty("executable_sha256",currentProgram.getExecutableSHA256());
        report.addProperty("language",currentProgram.getLanguageID().toString());
        report.addProperty("compiler_spec",currentProgram.getCompilerSpec().getCompilerSpecID().toString());
        JsonArray results=new JsonArray(); report.add("functions",results);
        Gson gson=new GsonBuilder().setPrettyPrinting().create();
        Path dest=Paths.get(args[1]);
        DecompInterface decompiler=new DecompInterface();
        decompiler.openProgram(currentProgram);
        int count=0;
        for(String line:Files.readAllLines(Paths.get(args[0]),StandardCharsets.UTF_8)) {
            monitor.checkCancelled();
            if(line.isBlank() || line.startsWith("#")) continue;
            if(++count>24) break;
            String[] parts=line.split("\t",3);
            Address start=toAddr(parts[0]); Address end=toAddr(parts[1]).subtract(1);
            if(end.subtract(start)<0 || end.subtract(start)>16000) continue;
            AddressSet body=new AddressSet(start,end);
            JsonObject item=new JsonObject(); results.add(item);
            item.addProperty("address",start.toString()); item.addProperty("end_inclusive",end.toString());
            item.addProperty("label_evidence",parts.length>2?parts[2]:"");
            println("AURA_TARGET "+start+" "+(parts.length>2?parts[2]:""));
            boolean decoded=new DisassembleCommand(start,body,true).applyTo(currentProgram,monitor);
            item.addProperty("disassembled",decoded);
            Function function=getFunctionAt(start);
            try {
                if(function==null) function=currentProgram.getFunctionManager().createFunction(
                    "audit_"+start.toString(),start,body,SourceType.USER_DEFINED);
            } catch(Exception error) { item.addProperty("create_error",error.toString()); }
            JsonArray instructions=new JsonArray(); JsonArray refs=new JsonArray(); JsonArray operands=new JsonArray();
            InstructionIterator iterator=currentProgram.getListing().getInstructions(body,true);
            int n=0;
            while(iterator.hasNext()) {
                Instruction instruction=iterator.next();
                if(++n<=240) instructions.add(instruction.getAddress()+" "+instruction.toString());
                // Ghidra represents RIP-relative LEA targets as Scalars rather than Address
                // objects on this x64 language. Preserve the decoded absolute operand explicitly.
                if(instruction.getMnemonicString().equals("LEA") && instruction.getNumOperands()>1) {
                    Matcher target=Pattern.compile("\\[0x([0-9a-fA-F]+)\\]").matcher(instruction.getDefaultOperandRepresentation(1));
                    if(target.matches()) {
                        JsonObject operand=new JsonObject();
                        operand.addProperty("from",instruction.getAddress().toString());
                        operand.addProperty("to",target.group(1));
                        operand.addProperty("operand",1);
                        operand.addProperty("instruction",instruction.toString());
                        operand.addProperty("source","Ghidra decoded absolute LEA operand representation");
                        operands.add(operand);
                    }
                }
                for(int op=0;op<instruction.getNumOperands();op++) {
                    for(Object object:instruction.getOpObjects(op)) {
                        if(object instanceof Address) {
                            JsonObject operand=new JsonObject();
                            operand.addProperty("from",instruction.getAddress().toString());
                            operand.addProperty("to",object.toString());
                            operand.addProperty("operand",op);
                            operand.addProperty("instruction",instruction.toString());
                            operands.add(operand);
                        }
                    }
                }
                for(var reference:instruction.getReferencesFrom()) {
                    JsonObject ref=new JsonObject();
                    ref.addProperty("from",reference.getFromAddress().toString());
                    ref.addProperty("to",reference.getToAddress().toString());
                    ref.addProperty("type",reference.getReferenceType().toString()); refs.add(ref);
                }
            }
            item.addProperty("instruction_count",n); item.add("first_240_instructions",instructions); item.add("references",refs);
            item.add("decoded_address_operands",operands);
            if(function!=null) {
                DecompileResults result=decompiler.decompileFunction(function,20,monitor);
                item.addProperty("decompiled",result.decompileCompleted());
                item.addProperty("decompiler_message",result.getErrorMessage());
                if(result.getDecompiledFunction()!=null) item.addProperty("pseudocode",result.getDecompiledFunction().getC());
            }
            Files.writeString(dest,gson.toJson(report),StandardCharsets.UTF_8);
        }
        decompiler.dispose();
        println("AURA_TARGETED_COMPLETE "+results.size());
    }
}
