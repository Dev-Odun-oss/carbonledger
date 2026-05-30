import { Module } from "@nestjs/common";
import { RetirementsController } from "./retirements.controller";
import { RetirementsService } from "./retirements.service";
import { PrismaService } from "../prisma.service";
import { AuthModule } from "../auth/auth.module";
import { CertificatesModule } from "../certificates/certificates.module";

@Module({
  imports: [AuthModule, CertificatesModule],
  controllers: [RetirementsController],
  providers: [RetirementsService, PrismaService],
})
export class RetirementsModule {}
