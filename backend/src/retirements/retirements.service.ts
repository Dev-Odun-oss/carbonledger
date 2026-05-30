import { Injectable, NotFoundException } from "@nestjs/common";
import { PrismaService } from "../prisma.service";

@Injectable()
export class RetirementsService {
  constructor(private readonly prisma: PrismaService) {}

  async findAll(limit = 20) {
    return this.prisma.retirementRecord.findMany({
      orderBy: { retiredAt: "desc" },
      take: limit,
    });
  }

  async findOne(retirementId: string) {
    const r = await this.prisma.retirementRecord.findUnique({ where: { retirementId } });
    if (!r) throw new NotFoundException(`Retirement ${retirementId} not found`);
    return r;
  }

  async getCertificate(retirementId: string) {
    const retirement = await this.findOne(retirementId);
    
    return {
      retirementId: retirement.retirementId,
      status: retirement.certificateStatus,
      cid: retirement.certificateCid,
      url: retirement.certificateUrl,
      generatedAt: retirement.certificateGeneratedAt,
      failedAt: retirement.certificateFailedAt,
      retries: retirement.certificateRetries,
    };
  }

  async generatePdf(retirementId: string): Promise<Buffer> {
    // PDF generation is handled asynchronously via queue
    // This endpoint returns the retirement data for reference
    const retirement = await this.findOne(retirementId);
    return Buffer.from(JSON.stringify(retirement));
  }
}
