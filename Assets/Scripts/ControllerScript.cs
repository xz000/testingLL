using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class ControllerScript : MonoBehaviour
{
    public GameObject PlayerCircle;
    public GameObject GroundCircle;
    GameObject[] thePC;
    MoveScript[] theMS;
    DoSkill[] theDS;

    public void powerrr(int PNO, List<ClickData> LCD)
    {
        foreach (ClickData CD in LCD)
        {
            if (CD.SC != null)
            {
                thePC[PNO].SendMessage("Go" + CD.SC.ToString());
                //Debug.Log(PNO + " Go" + CD.SC.ToString());
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
                //Debug.Log(PNO + " Get " + CD.blr.ToString() + " " + CD.xPos.ToString());
            }
            if (CD.gn != string.Empty)
            {
                Fix64Vector2 v2s = (Fix64Vector2)GameObject.Find(CD.gn).GetComponent<Rigidbody2D>().position;
                theDS[PNO].justdoit(v2s);
            }
        }
        LCD.Clear();
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
        CreateGround();
        Fix64Vector2 fix64Vector2 = new Fix64Vector2((Fix64)0, (Fix64)3);
        for (int i = MNum - 1; i >= 0; i--)
        {
            fix64Vector2 = fix64Vector2.CCWTurn(Fix64.PiTimes2 / (Fix64)MNum);
            thePC[i] = Instantiate(PlayerCircle, fix64Vector2.ToV2(), Quaternion.identity);
            thePC[i].name = i.ToString();
            theMS[i] = thePC[i].GetComponent<MoveScript>();
            theDS[i] = thePC[i].GetComponent<DoSkill>();
            thePC[i].GetComponent<HPScript>().ser = GetComponent<Sender>();
            thePC[i].GetComponent<CircleSkillMem>().SetCircleSL(Sender.theSLtemp[i]);
            if (i == Sender.clientNum)
                theMS[i].itsme();
        }
    }

    void CreateGround()
    {
        Instantiate(GroundCircle, Vector3.zero, Quaternion.identity);
    }
}
