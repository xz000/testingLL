using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class ControllerScript : MonoBehaviour
{
    public GameObject PlayerCircle;
    GameObject[] thePC;
    MoveScript[] theMS;
    DoSkill[] theDS;

    public void powerrr(int PNO, List<ClickData> LCD)
    {
        foreach(ClickData CD in LCD)
        {
            if (CD.SC != null)
            {
                switch (CD.SC)
                {
                    case SkillCode.TestSkill01:
                        thePC[PNO].GetComponent<TestSkill01>().Go();
                        break;
                }
            }
            if (CD.blr != null)
            {
                switch (CD.blr)
                {
                    case MButton.left:
                        Fix64Vector2 v2l = new Fix64Vector2((Fix64)CD.xPos, (Fix64)CD.yPos);
                        theDS[PNO].justdoit(v2l);
                        break;
                    case MButton.right:
                        Vector2 v2r = new Vector2((float)CD.xPos, (float)CD.yPos);
                        theMS[PNO].SetTarget(v2r);
                        break;
                }
            }
        }
        LCD.Clear();
    }

    public void CPCat(int Num, Vector2 place)
    {
        thePC[Num] = Instantiate(PlayerCircle, place, Quaternion.identity);
        theMS[Num] = thePC[Num].GetComponent<MoveScript>();
        theDS[Num] = thePC[Num].GetComponent<DoSkill>();
        if (Num == Sender.clientNum)
            theMS[Num].itsme();
    }

    public void createPCs(int MaxNum)
    {
        thePC = new GameObject[MaxNum];
        theMS = new MoveScript[MaxNum];
        theDS = new DoSkill[MaxNum];
        PCBorn(MaxNum);
    }

    void PCBorn(int MNum)
    {
        Vector3 v3;
        for (int i = 0; i < MNum; i++)
        {
            v3 = new Vector3(i * 3, 0, 0);
            CPCat(i, v3);
        }
    }
}
