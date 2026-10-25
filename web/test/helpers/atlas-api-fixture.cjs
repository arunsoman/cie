const fs=require('node:fs/promises'),path=require('node:path');
const java=`import org.springframework.web.bind.annotation.*;
import org.springframework.boot.autoconfigure.SpringBootApplication;
@SpringBootApplication
class App { public static void main(String[] args) {} }
@RestController
class Payments {
 @PostMapping("/pay")
 public int pay() { return save(); }
 private int save() { return audit(); }
 private int audit() { return 1; }
 @GetMapping("/refund")
 public int refund() { return gateway(); }
 public int gateway() { return 2; }
 @GetMapping("/ambiguous")
 public int overloaded(int x) { return x; }
 public int overloaded(String x) { return 0; }
}
`;
module.exports=async root=>{
 await fs.writeFile(path.join(root,'pom.xml'),'<project><modelVersion>4.0.0</modelVersion><groupId>qa</groupId><artifactId>payments</artifactId><version>1</version><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency></dependencies></project>');
 await fs.writeFile(path.join(root,'Payments.java'),java);
};
