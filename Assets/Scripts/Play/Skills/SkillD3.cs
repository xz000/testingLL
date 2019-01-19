using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillD3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed = 5;
    public GameObject Missile;
    public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
    }
	
	// Update is called once per frame
	void GoSkillD3()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        GameObject SkillTarget = FindClosestEnemy(actionplace);
        if (SkillTarget != null)
            actionplace = SkillTarget.transform.position;
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + 0.5f * skilldirection.normalized, skilldirection.normalized * bulletspeed, SkillTarget);
        currentcooldown = 0;
        skillavaliable = false;
    }

    GameObject FindClosestEnemy(Vector2 findingplace)
    {
        GameObject closest = null;  // GameObject.FindWithTag("Player");
        GameObject[] Allthem = GameObject.FindGameObjectsWithTag("Player");
        float sqrdis = Mathf.Infinity;
        foreach (GameObject Him in Allthem)
        {
            if (Him == gameObject)
                continue;//跳过施法者
            Vector2 diff = (Him.GetComponent<Rigidbody2D>().position - findingplace); //距离向量
            float curDistance = diff.sqrMagnitude; //距离平方
            if (curDistance < sqrdis)
            {
                closest = Him; //更新最近距离敌人
                sqrdis = curDistance; //更新最近距离
            }
        }
        return closest;
    }

    void DoFire(Vector2 fireplace, Vector2 speed2d, GameObject target)
    {
        GameObject bullet;
        Missile.GetComponent<BombExplode>().sender = gameObject;
        bullet = Instantiate(Missile, fireplace, Quaternion.identity);
        bullet.GetComponent<MissileScript>().Speed = bulletspeed;
        //bullet.GetComponent<BombExplode>().bombpower = force;
        bullet.GetComponent<BombExplode>().bombdamage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
        if (target != null)
            bullet.GetComponent<MissileScript>().Target = target.GetComponent<Rigidbody2D>();
        bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void SkillD3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
